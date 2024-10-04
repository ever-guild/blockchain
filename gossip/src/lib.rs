// 2022-2024 (c) Copyright Contributors to the GOSH DAO. All rights reserved.
//

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use chitchat::spawn_chitchat;
use chitchat::transport::UdpTransport;
use chitchat::Chitchat;
use chitchat::ChitchatConfig;
use chitchat::ChitchatHandle;
use chitchat::ChitchatId;
use chitchat::ClusterStateSnapshot;
use chitchat::FailureDetectorConfig;
use cool_id_generator::Size;
use poem::listener::TcpListener;
use poem::Route;
use poem::Server;
use poem_openapi::param::Query;
use poem_openapi::payload::Json;
use poem_openapi::OpenApi;
use poem_openapi::OpenApiService;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;

static DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse {
    pub cluster_id: String,
    pub cluster_state: ClusterStateSnapshot,
    pub live_nodes: Vec<ChitchatId>,
    pub dead_nodes: Vec<ChitchatId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetKeyValueResponse {
    pub status: bool,
}

struct Api {
    chitchat: Arc<Mutex<Chitchat>>,
}

#[OpenApi]
impl Api {
    /// Chitchat state
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> Json<serde_json::Value> {
        let chitchat_guard = self.chitchat.lock().await;
        let response = ApiResponse {
            cluster_id: chitchat_guard.cluster_id().to_string(),
            cluster_state: chitchat_guard.state_snapshot(),
            live_nodes: chitchat_guard.live_nodes().cloned().collect::<Vec<_>>(),
            dead_nodes: chitchat_guard.dead_nodes().cloned().map(|node| node.0).collect::<Vec<_>>(),
        };
        Json(serde_json::to_value(&response).unwrap())
    }

    /// Sets a key-value pair on this node (without validation).
    #[oai(path = "/set_kv/", method = "get")]
    async fn set_kv(&self, key: Query<String>, value: Query<String>) -> Json<serde_json::Value> {
        let mut chitchat_guard = self.chitchat.lock().await;

        let cc_state = chitchat_guard.self_node_state();
        cc_state.set(key.as_str(), value.as_str());

        Json(serde_json::to_value(&SetKeyValueResponse { status: true }).unwrap())
    }
}

fn generate_server_id(public_addr: SocketAddr) -> String {
    let cool_id = cool_id_generator::get_id(Size::Medium);
    format!("server:{public_addr}-{cool_id}")
}

pub async fn run(
    listen_addr: SocketAddr,
    gossip_advertise_addr: SocketAddr,
    seeds: Vec<String>,
) -> anyhow::Result<ChitchatHandle> {
    let node_id = generate_server_id(gossip_advertise_addr);
    let generation = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let chitchat_id = ChitchatId::new(node_id, generation, gossip_advertise_addr);
    let config = ChitchatConfig {
        cluster_id: "ackinacki".to_string(),
        chitchat_id,
        gossip_interval: DEFAULT_GOSSIP_INTERVAL,
        listen_addr,
        seed_nodes: seeds.clone(),
        failure_detector_config: FailureDetectorConfig::default(),
        marked_for_deletion_grace_period: 10_000,
    };
    tracing::info!("Starting UDP gossip server on {gossip_advertise_addr}");
    let chitchat_handler = spawn_chitchat(config, Vec::new(), &UdpTransport).await?;
    let chitchat = chitchat_handler.chitchat();
    let api = Api { chitchat: chitchat.clone() };
    let api_service = OpenApiService::new(api, "Acki Nacki", "1.0")
        .server(format!("http://{}/", gossip_advertise_addr));
    let docs = api_service.swagger_ui();
    let app = Route::new().nest("/", api_service).nest("/docs", docs);
    tracing::info!("Starting REST server on {gossip_advertise_addr}");
    tokio::spawn(Server::new(TcpListener::bind(gossip_advertise_addr)).run(app));

    Ok(chitchat_handler)
}
