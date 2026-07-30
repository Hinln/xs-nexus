use std::sync::Arc;

use tokio::{sync::watch, task::JoinHandle};

use crate::{
    config::AgentConfig,
    control::run_control_loop,
    data_plane::UdpDataPlane,
    error::{AgentError, Result},
    health::AgentHealth,
    ipc::{IpcContext, run_ipc_server},
    network::{NetworkPlan, TunNetwork},
    state::NodeState,
    storage::{Identity, read_json, write_json},
};

const MAX_IPV4_PACKET_BYTES: usize = 65_535;

struct AgentRuntime {
    config: AgentConfig,
    network: TunNetwork,
    data_plane: UdpDataPlane,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    candidate_sender: watch::Sender<Option<xs_core::CandidateAdvertisement>>,
    shutdown: watch::Receiver<bool>,
    control_task: JoinHandle<()>,
    ipc_task: JoinHandle<Result<()>>,
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
    let network = TunNetwork::create(plan.clone(), &config.network_manifest_path()).await?;
    let data_plane = UdpDataPlane::bind(&state, Arc::clone(&identity)).await?;
    let data_plane_status = data_plane.status_handle();
    let state = Arc::new(tokio::sync::RwLock::new(state));
    let health = Arc::new(AgentHealth::new());
    let (candidate_sender, candidate_receiver) = watch::channel(None);

    let control_task = tokio::spawn(run_control_loop(
        config.clone(),
        Arc::clone(&identity),
        Arc::clone(&state),
        Arc::clone(&health),
        candidate_receiver,
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
        shutdown,
        control_task,
        ipc_task,
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
                        &mut self.data_plane,
                        &self.state,
                        &self.config,
                        &self.candidate_sender,
                    ).await {
                        report_runtime_error("data_plane_maintenance", &error);
                        break Err(error);
                    }
                }
                result = &mut self.control_task, if !control_completed => {
                    control_completed = true;
                    if *self.shutdown.borrow() && result.is_ok() {
                        break Ok(());
                    }
                    break Err(AgentError::Runtime);
                }
                result = &mut self.ipc_task, if !ipc_completed => {
                    ipc_completed = true;
                    match result {
                        Ok(Ok(())) if *self.shutdown.borrow() => break Ok(()),
                        Ok(Err(error)) => break Err(error),
                        Err(_) | Ok(Ok(())) => break Err(AgentError::Runtime),
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
}

async fn maintain_data_plane(
    data_plane: &mut UdpDataPlane,
    state: &tokio::sync::RwLock<NodeState>,
    config: &AgentConfig,
    candidate_sender: &watch::Sender<Option<xs_core::CandidateAdvertisement>>,
) -> Result<()> {
    let snapshot = state.read().await.clone();
    if data_plane.configuration_version() != snapshot.configuration.version {
        data_plane.apply_configuration(&snapshot).await?;
    }
    if let Some(advertisement) = data_plane.maintain(&snapshot).await? {
        {
            let mut state = state.write().await;
            state.candidate_generation = advertisement.generation;
            write_json(&config.node_state_path(), &*state)?;
        }
        candidate_sender.send_replace(Some(advertisement));
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
