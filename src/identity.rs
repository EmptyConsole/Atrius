use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

use crate::model::DeviceId;

pub type UserId = Ulid;
pub type SessionId = Ulid;

/// Device-authenticated identity. Keys are represented generically to avoid
/// binding to a crypto library here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub user_id: UserId,
    pub device_public_key: Vec<u8>, // e.g., Ed25519 public key bytes
    pub attested_at: SystemTime,
}

/// User authentication token (opaque bearer or signed proof).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAuthToken {
    pub user_id: UserId,
    pub issued_at: SystemTime,
    pub expires_at: SystemTime,
    pub token: Vec<u8>,
}

/// Advertised peer info used for discovery and connection attempts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerAdvertisement {
    pub device_id: DeviceId,
    pub user_id: UserId,
    pub session_id: SessionId,
    pub addresses: Vec<SocketAddr>, // preferred: direct P2P (LAN/public)
    pub relays: Vec<RelayHint>,     // fallback relays
    pub advertised_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayHint {
    pub relay_id: Ulid,
    pub url: String, // e.g., wss://relay.example.com
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionPath {
    PeerToPeer(SocketAddr),
    Relay { relay: RelayHint, via: SocketAddr },
}

/// Result of attempting to resolve the best path to a peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSelection {
    pub target: DeviceId,
    pub chosen: Option<ConnectionPath>,
    pub attempted: Vec<ConnectionPath>,
}

/// Configuration knobs for discovery and connection preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub prefer_p2p: bool,
    pub relay_timeout: Duration,
    pub max_advert_age: Duration,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("authentication expired")]
    AuthExpired,
    #[error("no viable path to peer")]
    NoPath,
}

impl UserAuthToken {
    pub fn is_valid(&self, now: SystemTime) -> Result<(), IdentityError> {
        if now >= self.expires_at {
            return Err(IdentityError::AuthExpired);
        }
        Ok(())
    }
}

/// Select a preferred connection path given a peer advertisement and a config.
/// Preference: direct P2P addresses first; if none, fall back to relays.
pub fn choose_path(
    advert: &PeerAdvertisement,
    config: &DiscoveryConfig,
) -> Result<PathSelection, IdentityError> {
    let mut attempted = Vec::new();

    if config.prefer_p2p {
        if let Some(addr) = advert.addresses.first() {
            let path = ConnectionPath::PeerToPeer(*addr);
            attempted.push(path.clone());
            return Ok(PathSelection {
                target: advert.device_id,
                chosen: Some(path),
                attempted,
            });
        }
    }

    if let Some(relay) = advert.relays.first() {
        if let Some(addr) = advert.addresses.first() {
            let path = ConnectionPath::Relay {
                relay: relay.clone(),
                via: *addr,
            };
            attempted.push(path.clone());
            return Ok(PathSelection {
                target: advert.device_id,
                chosen: Some(path),
                attempted,
            });
        } else {
            let path = ConnectionPath::Relay {
                relay: relay.clone(),
                via: "0.0.0.0:0".parse().unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()),
            };
            attempted.push(path.clone());
            return Ok(PathSelection {
                target: advert.device_id,
                chosen: Some(path),
                attempted,
            });
        }
    }

    Err(IdentityError::NoPath)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_token_validity() {
        let now = SystemTime::now();
        let token = UserAuthToken {
            user_id: Ulid::new(),
            issued_at: now,
            expires_at: now + Duration::from_secs(60),
            token: vec![1, 2, 3],
        };
        assert!(token.is_valid(now).is_ok());
        assert!(token
            .is_valid(now + Duration::from_secs(61))
            .is_err());
    }

    #[test]
    fn choose_p2p_if_available() {
        let advert = PeerAdvertisement {
            device_id: Ulid::new(),
            user_id: Ulid::new(),
            session_id: Ulid::new(),
            addresses: vec!["10.0.0.2:7777".parse().unwrap()],
            relays: vec![RelayHint {
                relay_id: Ulid::new(),
                url: "wss://relay.example.com".into(),
            }],
            advertised_at: SystemTime::now(),
        };
        let cfg = DiscoveryConfig {
            prefer_p2p: true,
            relay_timeout: Duration::from_secs(5),
            max_advert_age: Duration::from_secs(60),
        };
        let path = choose_path(&advert, &cfg).unwrap();
        matches!(path.chosen, Some(ConnectionPath::PeerToPeer(_)));
    }

    #[test]
    fn fall_back_to_relay() {
        let advert = PeerAdvertisement {
            device_id: Ulid::new(),
            user_id: Ulid::new(),
            session_id: Ulid::new(),
            addresses: vec![],
            relays: vec![RelayHint {
                relay_id: Ulid::new(),
                url: "wss://relay.example.com".into(),
            }],
            advertised_at: SystemTime::now(),
        };
        let cfg = DiscoveryConfig {
            prefer_p2p: true,
            relay_timeout: Duration::from_secs(5),
            max_advert_age: Duration::from_secs(60),
        };
        let path = choose_path(&advert, &cfg).unwrap();
        matches!(path.chosen, Some(ConnectionPath::Relay { .. }));
    }
}
