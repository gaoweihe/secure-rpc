include!("common.rs");

#[allow(unused_imports)]
use tracing::info;

use serde::Deserialize;
use serde::Serialize;

use precomm_grpc::*;
pub mod precomm_grpc {
    tonic::include_proto!("pre_comm");
}
use precomm_grpc::pre_comm_service_client::*; 

static CONTEXT: once_cell::sync::OnceCell<ibverbs::Context> = 
    once_cell::sync::OnceCell::new(); 
static PROTECTION_DOMAIN: once_cell::sync::OnceCell<std::sync::Arc<ibverbs::ProtectionDomain>> = 
    once_cell::sync::OnceCell::new(); 
static RECEIVE_QUEUE: once_cell::sync::OnceCell<ibverbs::CompletionQueue> =
    once_cell::sync::OnceCell::new();
static SEND_QUEUE: once_cell::sync::OnceCell<ibverbs::CompletionQueue> =
    once_cell::sync::OnceCell::new();
static LOCAL_ENDPOINT: once_cell::sync::OnceCell<std::sync::Arc<Vec<u8>>> = 
    once_cell::sync::OnceCell::new();
static REMOTE_ENDPOINT: once_cell::sync::OnceCell<std::sync::Arc<Vec<u8>>> = 
    once_cell::sync::OnceCell::new();

#[allow(unused_variables)]
#[tokio::main]
async fn main() {
    // set tracer 
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed"); 

    CONTEXT.set(
        ibverbs::devices()
            .unwrap()
            .iter()
            .next()
            .expect("no rdma device available")
            .open()
            .unwrap()
    ).unwrap(); 
    let ctx = CONTEXT.get().unwrap();

    PROTECTION_DOMAIN.set(
        std::sync::Arc::new(ctx.alloc_pd().unwrap())
    ).unwrap();
    let pd = PROTECTION_DOMAIN.get().unwrap();

    RECEIVE_QUEUE.set(
        ctx.create_cq(32, 0).unwrap()
    ).unwrap();
    let rq = RECEIVE_QUEUE.get().unwrap();

    SEND_QUEUE.set(
        ctx.create_cq(32, 0).unwrap()
    ).unwrap();
    let sq = SEND_QUEUE.get().unwrap();

    let qp_builder = pd.create_qp(
        &sq, 
        8, 
        &rq, 
        8, 
        ibverbs::ibv_qp_type::IBV_QPT_RC
    ).build().unwrap();

    // get local endpoint 
    let endpoint = qp_builder.endpoint();
    info!("local endpoint: {:?}", endpoint);

    // serialize local endpoint 
    let ep_bin = serialize_endpoint(endpoint);
    LOCAL_ENDPOINT.set(
        std::sync::Arc::new(ep_bin)
    ).unwrap(); 

    // send grpc request to get remote endpoint 
    let mut client = 
        PreCommServiceClient::connect("http://[::1]:50051")
            .await.unwrap();
    let request = tonic::Request::new(GetEndpointRequest {
        src_endpoint: LOCAL_ENDPOINT.get().unwrap().to_vec(),
    });
    let response = client.get_endpoint(request).await.unwrap();
    let rmt_ep_vec = response.into_inner().endpoint;
    REMOTE_ENDPOINT.set(
        std::sync::Arc::new(rmt_ep_vec.clone())
    ).unwrap();

    // deserialize remote endpoint 
    let rmt_ep = deserialize_endpoint(rmt_ep_vec);
    info!("remote endpoint: {:?}", rmt_ep);

    // handshake with remote endpoint 
    let mut qp = qp_builder.handshake(rmt_ep).unwrap();

    let push_handle = tokio::spawn(async move {
        let mut mr = pd.allocate::<u8>(2).unwrap();

        let mut wr_id = 10000000;

        loop {
            let result = unsafe { 
                qp.post_send(&mut mr, .., wr_id) 
            };
            match result {
                Ok(_) => {
                    wr_id += 1;
                    // info!("post_send: OK: wr_id = {}", wr_id);
                },
                Err(e) => {
                    // info!("post_send: Err: {:?}", e);
                }
            }
        }
    });

    let sq_poll = sq.clone();
    let poll_handle = tokio::spawn(async move {
        let mut completions = [ibverbs::ibv_wc::default(); 32];

        let mut req_cnt = 0;

        loop {
            let completed = sq_poll.poll(&mut completions[..]).unwrap();
            if completed.is_empty() {
                continue;
            }
            for wc in completed {
                match wc.opcode() {
                    ibverbs::ibv_wc_opcode::IBV_WC_SEND => {
                        req_cnt += 1;
                        if req_cnt >= 1000 {
                            std::process::exit(0x0000);
                        }
                    }
                    _ => {
                        panic!("unexpected completion code {:?}", wc.opcode());
                    },
                }
            }
        }
    }); 

    push_handle.await.unwrap();
    poll_handle.await.unwrap();
}