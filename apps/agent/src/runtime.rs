use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};

use crate::{
    config::AgentConfig,
    control::{ControlContext, run_control_loop},
    data_plane::{ManualProbeError, UdpDataPlane},
    error::{AgentError, Result},
    health::AgentHealth,
    ipc::{IpcContext, RuntimeCommand, RuntimeCommandError, run_ipc_server},
    network::{NetworkPlan, TunNetwork},
    state::NodeState,
    storage::{Identity, read_json, write_json},
    subnet_routes::SubnetRouteDiscovery,
};

const MAX_IPV4_PACKET_BYTES: usize = 65_535;
const RUNTIME_COMMAND_CAPACITY: usize = 16;
const MANUAL_RECONNECT_COOLDOWN: Duration = Duration::from_secs(1);

struct AgentRuntime {
    config: AgentConfig,
    network: TunNetwork,
    data_plane: UdpDataPlane,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    candidate_sender: watch::Sender<Option<xs_core::CandidateAdvertisement>>,
    subnet_route_sender: watch::Sender<Option<xs_core::SubnetRouteAdvertisement>>,
    subnet_route_discovery: SubnetRouteDiscovery,
    shutdown: watch::Receiver<bool>,
    control_task: JoinHandle<()>,
    ipc_task: JoinHandle<Result<()>>,
    runtime_commands: mpsc::Receiver<RuntimeCommand>,
    control_reconnect_sender: watch::Sender<u64>,
    last_manual_reconnect: Option<Instant>,
}

/// Runs the Linux Agent until shutdown is requested or a required local subsystem fails.
///
/// # Errors
///
/// Returns an Agent error when trusted state cannot be loaded, the TUN lifecycle fails,
/// or the local management socket terminates unexpectedly.
pub async fn run_agent(config: AgentConfig, shutdown: watch::Receiver<bool>) -> Result<()> {
    config.validate()?;
    let identity = Arc::new(Identity::load_or_create(&config.identity_path())?);
    let state: NodeState = read_json(&config.node_state_path())?;
    let controller_url = config.controller_url()?;
    state.validate(&identity, controller_url.as_str())?;
    let plan = NetworkPlan::from_state(&config, &state)?;
    let network = TunNetwork::create(
        plan.clone(),
        &config.network_manifest_path(),
        &config.network_manifest_temporary_path(),
        config.windows_wintun.as_ref(),
    )
    .await?;
    let data_plane = UdpDataPlane::bind(&state, Arc::clone(&identity)).await?;
    let data_plane_status = data_plane.status_handle();
    let mut telemetry_boot_id = [0_u8; 16];
    getrandom::fill(&mut telemetry_boot_id).map_err(|_| AgentError::State)?;
    let telemetry_boot_id_base64 = URL_SAFE_NO_PAD.encode(telemetry_boot_id);
    let state = Arc::new(tokio::sync::RwLock::new(state));
    let health = Arc::new(AgentHealth::new());
    let (candidate_sender, candidate_receiver) = watch::channel(None);
    let (subnet_route_sender, subnet_route_receiver) = watch::channel(None);
    let (runtime_command_sender, runtime_commands) = mpsc::channel(RUNTIME_COMMAND_CAPACITY);
    let (control_reconnect_sender, control_reconnect_receiver) = watch::channel(0_u64);
    let subnet_route_discovery = {
        let state = state.read().await;
        SubnetRouteDiscovery::new(&state, plan.interface_name().to_owned())?
    };

    let control_task = tokio::spawn(run_control_loop(
        ControlContext {
            config: config.clone(),
            identity: Arc::clone(&identity),
            state: Arc::clone(&state),
            health: Arc::clone(&health),
            data_plane_status: Arc::clone(&data_plane_status),
            telemetry_boot_id_base64,
        },
        candidate_receiver,
        subnet_route_receiver,
        control_reconnect_receiver,
        shutdown.clone(),
    ));
    let ipc_task = tokio::spawn(run_ipc_server(
        config.socket_path(),
        Arc::clone(&state),
        Arc::clone(&health),
        IpcContext {
            interface_name: plan.interface_name().to_owned(),
            interface_index: network.interface_index(),
            data_plane_status,
            runtime_commands: runtime_command_sender,
        },
        shutdown.clone(),
    ));

    AgentRuntime {
        config,
        network,
        data_plane,
        state,
        health,
        candidate_sender,
        subnet_route_sender,
        subnet_route_discovery,
        shutdown,
        control_task,
        ipc_task,
        runtime_commands,
        control_reconnect_sender,
        last_manual_reconnect: None,
    }
    .run()
    .await
}

impl AgentRuntime {
    async fn run(mut self) -> Result<()> {
        let mut control_completed = false;
        let mut ipc_completed = false;
        let mut packet = vec![0_u8; MAX_IPV4_PACKET_BYTES];
        let mut data_plane_maintenance =
            tokio::time::interval(std::time::Duration::from_millis(100));
        data_plane_maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let runtime_result = loop {
            tokio::select! {
                result = self.shutdown.changed() => {
                    if result.is_err() || *self.shutdown.borrow() {
                        break Ok(());
                    }
                }
                received = self.network.receive(&mut packet) => {
                    match received {
                        Ok(0) => break Err(AgentError::Network),
                        Ok(length) => {
                            let dropped = match self.data_plane.forward_tun(&packet[..length]).await {
                                Ok(dropped) => dropped,
                                Err(error) => {
                                    report_runtime_error("tun_forward", &error);
                                    break Err(error);
                                }
                            };
                            self.health.record_tun_packet(dropped);
                        }
                        Err(error) => {
                            report_runtime_error("tun_receive", &error);
                            break Err(error);
                        }
                    }
                }
                received = self.data_plane.receive() => {
                    match received {
                        Ok(Some(packet)) => {
                            if let Err(error) = self.network.send(&packet).await {
                                report_runtime_error("tun_send", &error);
                                break Err(error);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            report_runtime_error("udp_receive", &error);
                            break Err(error);
                        }
                    }
                }
                _ = data_plane_maintenance.tick() => {
                    if let Err(error) = maintain_data_plane(
                        &mut self.network,
                        &mut self.data_plane,
                        &self.state,
                        &self.config,
                        &self.candidate_sender,
                        &mut self.subnet_route_discovery,
                        &self.subnet_route_sender,
                    ).await {
                        report_runtime_error("data_plane_maintenance", &error);
                        break Err(error);
                    }
                }
                command = self.runtime_commands.recv() => {
                    let Some(command) = command else {
                        report_runtime_error("runtime_command_channel", &AgentError::Runtime);
                        break Err(AgentError::Runtime);
                    };
                    self.handle_runtime_command(command).await;
                }
                result = &mut self.control_task, if !control_completed => {
                    control_completed = true;
                    if *self.shutdown.borrow() && result.is_ok() {
                        break Ok(());
                    }
                    break Err(runtime_task_error("control_task", AgentError::Runtime));
                }
                result = &mut self.ipc_task, if !ipc_completed => {
                    ipc_completed = true;
                    match result {
                        Ok(Ok(())) if *self.shutdown.borrow() => break Ok(()),
                        Ok(Err(error)) => break Err(runtime_task_error("ipc_task", error)),
                        Err(_) | Ok(Ok(())) => {
                            break Err(runtime_task_error("ipc_task", AgentError::Runtime));
                        }
                    }
                }
            }
        };

        if !control_completed {
            stop_task(self.control_task).await;
        }
        if !ipc_completed {
            stop_task(self.ipc_task).await;
        }
        let network_result = self.network.shutdown().await;
        runtime_result.and(network_result)
    }

    async fn handle_runtime_command(&mut self, command: RuntimeCommand) {
        match command {
            RuntimeCommand::Probe {
                virtual_ip,
                response,
            } => {
                let result = self
                    .data_plane
                    .start_manual_path_probe(virtual_ip)
                    .await
                    .map_err(map_probe_error);
                let _ = response.send(result);
            }
            RuntimeCommand::Reconnect { response } => {
                let now = Instant::now();
                let result = if self.last_manual_reconnect.is_some_and(|last| {
                    now.saturating_duration_since(last) < MANUAL_RECONNECT_COOLDOWN
                }) {
                    Err(RuntimeCommandError::RateLimited)
                } else {
                    self.last_manual_reconnect = Some(now);
                    self.control_reconnect_sender
                        .send_modify(|generation| *generation = generation.wrapping_add(1));
                    Ok(())
                };
                let _ = response.send(result);
            }
        }
    }
}

const fn map_probe_error(error: ManualProbeError) -> RuntimeCommandError {
    match error {
        ManualProbeError::PeerNotFound => RuntimeCommandError::PeerNotFound,
        ManualProbeError::SessionUnavailable => RuntimeCommandError::SessionUnavailable,
        ManualProbeError::Busy => RuntimeCommandError::Busy,
        ManualProbeError::SendFailed => RuntimeCommandError::Network,
    }
}

async fn maintain_data_plane(
    network: &mut TunNetwork,
    data_plane: &mut UdpDataPlane,
    state: &tokio::sync::RwLock<NodeState>,
    config: &AgentConfig,
    candidate_sender: &watch::Sender<Option<xs_core::CandidateAdvertisement>>,
    subnet_route_discovery: &mut SubnetRouteDiscovery,
    subnet_route_sender: &watch::Sender<Option<xs_core::SubnetRouteAdvertisement>>,
) -> Result<()> {
    let snapshot = state.read().await.clone();
    if data_plane.configuration_version() != snapshot.configuration.version {
        data_plane.apply_configuration(&snapshot).await?;
    }
    network.reconcile_subnet_routes(&snapshot).await?;
    if let Some(advertisement) = data_plane.maintain(&snapshot).await? {
        {
            let mut state = state.write().await;
            state.candidate_generation = advertisement.generation;
            write_json(&config.node_state_path(), &*state)?;
        }
        candidate_sender.send_replace(Some(advertisement));
    }
    if subnet_route_discovery.refresh_due(std::time::Instant::now()) {
        let advertisement = subnet_route_discovery.refresh().await?;
        {
            let mut state = state.write().await;
            state.subnet_route_generation = advertisement.generation;
            write_json(&config.node_state_path(), &*state)?;
        }
        subnet_route_sender.send_replace(Some(advertisement));
    }
    Ok(())
}

async fn stop_task<T>(mut task: JoinHandle<T>) {
    if tokio::time::timeout(std::time::Duration::from_secs(5), &mut task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

fn report_runtime_error(subsystem: &str, error: &AgentError) {
    eprintln!("xs-agent subsystem={subsystem} error={}", error.code());
}

fn runtime_task_error(subsystem: &str, error: AgentError) -> AgentError {
    report_runtime_error(subsystem, &error);
    error
}
