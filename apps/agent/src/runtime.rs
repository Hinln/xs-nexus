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

/// Runs the Linux Agent until shutdown is requested or a required local subsystem fails.
///
/// # Errors
///
/// Returns an Agent error when trusted state cannot be loaded, the TUN lifecycle fails,
/// or the local management socket terminates unexpectedly.
pub async fn run_agent(config: AgentConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    config.validate()?;
    let identity = Arc::new(Identity::load_or_create(&config.identity_path())?);
    let state: NodeState = read_json(&config.node_state_path())?;
    let controller_url = config.controller_url()?;
    state.validate(&identity, controller_url.as_str())?;
    let plan = NetworkPlan::from_state(&config, &state)?;
    let network = TunNetwork::create(plan.clone(), &config.network_manifest_path()).await?;
    let mut data_plane = UdpDataPlane::bind(&state, Arc::clone(&identity)).await?;
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

    let mut control_task = control_task;
    let mut ipc_task = ipc_task;
    let mut control_completed = false;
    let mut ipc_completed = false;
    let mut packet = vec![0_u8; MAX_IPV4_PACKET_BYTES];
    let mut data_plane_maintenance = tokio::time::interval(std::time::Duration::from_millis(100));
    data_plane_maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let runtime_result = loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    break Ok(());
                }
            }
            received = network.receive(&mut packet) => {
                match received {
                    Ok(0) => break Err(AgentError::Network),
                    Ok(length) => {
                        let dropped = data_plane.forward_tun(&packet[..length]).await?;
                        health.record_tun_packet(dropped);
                    }
                    Err(error) => break Err(error),
                }
            }
            received = data_plane.receive() => {
                match received {
                    Ok(Some(packet)) => network.send(&packet).await?,
                    Ok(None) => {}
                    Err(error) => break Err(error),
                }
            }
            _ = data_plane_maintenance.tick() => {
                maintain_data_plane(
                    &mut data_plane,
                    &state,
                    &config,
                    &candidate_sender,
                ).await?;
            }
            result = &mut control_task, if !control_completed => {
                control_completed = true;
                if *shutdown.borrow() && result.is_ok() {
                    break Ok(());
                }
                break Err(AgentError::Runtime);
            }
            result = &mut ipc_task, if !ipc_completed => {
                ipc_completed = true;
                match result {
                    Ok(Ok(())) if *shutdown.borrow() => break Ok(()),
                    Ok(Err(error)) => break Err(error),
                    Err(_) | Ok(Ok(())) => break Err(AgentError::Runtime),
                }
            }
        }
    };

    if !control_completed {
        stop_task(control_task).await;
    }
    if !ipc_completed {
        stop_task(ipc_task).await;
    }
    let network_result = network.shutdown().await;
    runtime_result.and(network_result)
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
