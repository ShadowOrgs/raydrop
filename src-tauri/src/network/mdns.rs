use crate::db::storage::{now_unix_secs, Contact};
use crate::state::AppState;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_raydrop._tcp.local.";

pub fn start_mdns_daemon(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let Ok(mdns) = ServiceDaemon::new() else {
            warn!("Failed to initialize mDNS daemon");
            return;
        };

        let settings = state.settings.lock().unwrap().clone();
        let instance_name = format!(
            "{}-{}",
            settings.alias_name.replace(' ', "-"),
            &state.identity.device_id[4..8]
        );
        let host_name = format!("{}.local.", instance_name);

        let mut properties = HashMap::new();
        properties.insert("device_id".to_string(), state.identity.device_id.clone());
        properties.insert("alias".to_string(), settings.alias_name.clone());
        properties.insert("pubkey".to_string(), state.identity.public_hex());
        properties.insert("os".to_string(), std::env::consts::OS.to_string());
        if let Some(ref email) = settings.user_email {
            properties.insert("email".to_string(), email.clone());
        }
        if let Some(ref name) = settings.user_name {
            properties.insert("user_name".to_string(), name.clone());
        }
        if let Ok(Some(av_hash)) = state.db.get_setting("avatar_hash") {
            properties.insert("avatar_hash".to_string(), av_hash);
        }
        if let Ok(exp) = state.db.compute_my_rank_exp() {
            properties.insert("rank_exp".to_string(), exp.to_string());
        }
        if let Some(mac) = crate::commands::get_local_mac_address(&state.local_ip) {
            properties.insert("mac".to_string(), mac);
        }

        let build_service_info = || {
            ServiceInfo::new(
                SERVICE_TYPE,
                &instance_name,
                &host_name,
                &state.local_ip,
                state.listen_port,
                Some(properties.clone()),
            )
            .ok()
            .map(|s| s.enable_addr_auto())
        };

        let mut registered_fullname: Option<String> = None;
        if state.is_discoverable() {
            if let Some(service_info) = build_service_info() {
                let fullname = service_info.get_fullname().to_string();
                if let Err(e) = mdns.register(service_info) {
                    warn!("mDNS service registration failed: {e}");
                } else {
                    registered_fullname = Some(fullname);
                    info!(
                        "mDNS registered: {} on {}:{}",
                        state.identity.device_id, state.local_ip, state.listen_port
                    );
                }
            }
        }

        let Ok(receiver) = mdns.browse(SERVICE_TYPE) else {
            warn!("Failed to start mDNS browse");
            return;
        };

        loop {
            let currently_discoverable = state.is_discoverable();
            if currently_discoverable && registered_fullname.is_none() {
                if let Some(service_info) = build_service_info() {
                    let fullname = service_info.get_fullname().to_string();
                    if mdns.register(service_info).is_ok() {
                        registered_fullname = Some(fullname);
                        info!("mDNS re-registered (Visible mode)");
                    }
                }
            } else if !currently_discoverable {
                if let Some(fullname) = registered_fullname.take() {
                    let _ = mdns.unregister(&fullname);
                    info!("mDNS unregistered (Hidden/Offline mode)");
                }
            }

            let event = match receiver.recv_timeout(std::time::Duration::from_millis(800)) {
                Ok(ev) => ev,
                Err(flume_or_timeout) => {
                    if flume_or_timeout.to_string().contains("disconnected") {
                        break;
                    }
                    continue;
                }
            };
            if let ServiceEvent::ServiceResolved(info) = event {
                let Some(peer_id) = info.get_property_val_str("device_id") else {
                    continue;
                };
                if peer_id == state.identity.device_id {
                    continue;
                }

                let alias = info
                    .get_property_val_str("alias")
                    .unwrap_or("Unknown-LAN-Peer")
                    .to_string();
                let pubkey = info.get_property_val_str("pubkey").map(|s| s.to_string());
                let peer_email = info.get_property_val_str("email").map(|s| s.to_string());
                let peer_user_name = info
                    .get_property_val_str("user_name")
                    .map(|s| s.to_string());
                let peer_os = info.get_property_val_str("os").map(|s| s.to_string());
                let port = info.get_port();
                let ip = info
                    .get_addresses()
                    .iter()
                    .find(|ip| ip.is_ipv4())
                    .or_else(|| info.get_addresses().iter().next())
                    .map(|ip| ip.to_string());

                if let Some(ip_str) = ip {
                    let my_email = state.settings.lock().unwrap().user_email.clone();
                    let is_own_device = match (&my_email, &peer_email) {
                        (Some(me), Some(pe)) if !me.is_empty() => me.eq_ignore_ascii_case(pe),
                        _ => false,
                    };

                    let existing = state.db.get_contact(peer_id).ok().flatten();
                    let existing_trust = existing
                        .as_ref()
                        .map(|c| c.trust_level.clone())
                        .unwrap_or_else(|| {
                            if is_own_device {
                                "trusted_auto_accept".to_string()
                            } else {
                                "ask".to_string()
                            }
                        });

                    let peer_avatar_hash = info.get_property_val_str("avatar_hash").map(|s| s.to_string());
                    let peer_rank_exp = info
                        .get_property_val_str("rank_exp")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let peer_mac = if is_own_device {
                        info.get_property_val_str("mac").map(|s| s.to_string())
                    } else {
                        None
                    };
                    let contact = Contact {
                        device_id: peer_id.to_string(),
                        alias_name: alias,
                        custom_name: existing.as_ref().and_then(|c| c.custom_name.clone()),
                        user_email: peer_email.or_else(|| existing.as_ref().and_then(|c| c.user_email.clone())),
                        user_name: peer_user_name.or_else(|| existing.as_ref().and_then(|c| c.user_name.clone())),
                        os_type: peer_os.or_else(|| existing.as_ref().and_then(|c| c.os_type.clone())),
                        is_own_device: is_own_device || existing.as_ref().map(|c| c.is_own_device).unwrap_or(false),
                        ed25519_pubkey: pubkey,
                        network_type: "lan".to_string(),
                        last_known_ip: Some(ip_str),
                        last_known_port: Some(port),
                        trust_level: existing_trust,
                        is_online: true,
                        last_seen_at: now_unix_secs(),
                        created_at: now_unix_secs(),
                        avatar_hash: peer_avatar_hash.or_else(|| existing.as_ref().and_then(|c| c.avatar_hash.clone())),
                        rank_exp: peer_rank_exp.max(existing.as_ref().map(|c| c.rank_exp).unwrap_or(0)),
                        mac_address: peer_mac.or_else(|| existing.as_ref().and_then(|c| c.mac_address.clone())),
                    };

                    let _ = state.db.upsert_contact(&contact);
                    if contact.avatar_hash.is_some() {
                        crate::commands::request_peer_avatar_internal(&state, &contact.device_id);
                    }
                    let _ = app.emit("peer_discovered", &contact);
                }
            }
        }
    });
}
