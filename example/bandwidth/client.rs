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

#[derive(Debug, Clone)]
pub struct RpcConf {
    pub rmt_grpc_uri: String, 
}

impl RpcConf {
    pub fn new() -> Self {
        let rmt_grpc_uri = String::new();
        let conf = Self {
            rmt_grpc_uri, 
        };
        conf
    }
}

pub static RPC_CONF: once_cell::sync::OnceCell<RpcConf> = 
    once_cell::sync::OnceCell::new();

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

fn parse_args() {
    use argparse::{ ArgumentParser, Store }; 

    let mut conf = RpcConf::new();

    {
        let mut ap = ArgumentParser::new(); 
        ap.set_description("Secure RPC service. ");

        ap.refer(&mut conf.rmt_grpc_uri)
            .add_option(
                &["--server"], 
                Store, 
                "Remote gRPC URI. "
            );
        
        ap.parse_args_or_exit(); 
    }

    RPC_CONF.get_or_init(|| {
        conf
    });
}

#[allow(unused_variables)]
#[tokio::main]
async fn main() {
    // set tracer 
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed"); 

    // parse CLI-input arguments 
    parse_args();
    info!("RPC_CONF: {:?}", RPC_CONF.get().unwrap());

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
        ctx.create_cq(8, 0).unwrap()
    ).unwrap();
    let rq = RECEIVE_QUEUE.get().unwrap();

    SEND_QUEUE.set(
        ctx.create_cq(1024, 0).unwrap()
    ).unwrap();
    let sq = SEND_QUEUE.get().unwrap();
    let rq = RECEIVE_QUEUE.get().unwrap();

    let qp_builder = pd.create_qp(
        &sq, 
        1024, 
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
    let conf = RPC_CONF.get().unwrap();
    let rmt_uri = conf.rmt_grpc_uri.clone();
    let client_res = 
        PreCommServiceClient::connect(rmt_uri)
            .await;
    
    let mut client;
    loop {
        match client_res {
            Ok(client_some) => {
                client = client_some;
                break;
            },
            Err(ref e) => {
                info!("connect to remote gRPC server failed: {:?}", e);
            }
        }

        // sleep for one second 
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

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

    // let push_handle = tokio::spawn(async move {
    //     let mut mr = pd.allocate::<u8>(1048576).unwrap();

    //     let mut wr_id = 10000000;

    //     // record current time 
    //     let start = std::time::Instant::now();

    //     loop {
    //         let result = unsafe { 
    //             qp.post_send(&mut mr, .., wr_id) 
    //         };
    //         match result {
    //             Ok(_) => {
    //                 wr_id += 1;
    //                 // info!("post_send: OK: wr_id = {}", wr_id);
    //                 if wr_id >= 10000000 + 1000 {
    //                     break;
    //                 }  
    //             }, 
    //             Err(e) => {
    //                 // info!("post_send: Err: {:?}", e);
    //             }
    //         }
    //     }

    //     // record current time
    //     let end = std::time::Instant::now();

    //     // calculate time elapsed
    //     let elapsed = end - start;

    //     // print time elapsed
    //     info!("elapsed: {:?}", elapsed);

    //     // sleep for one second
    //     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    // });

    let sq_poll = sq.clone();
    let rq_poll = rq.clone();
    // let poll_handle = tokio::spawn(async move {
    //     let mut completions = [ibverbs::ibv_wc::default(); 100];

    //     let mut is_start = false;
    //     let mut req_cnt = 0;

    //     let mut start = std::time::Instant::now();

    //     let mut exit_flag = false;

    //     loop {
    //         let completed = rq_poll.poll(&mut completions[..]).unwrap();
    //         let completed = sq_poll.poll(&mut completions[..]).unwrap();
    //         if completed.is_empty() {
    //             continue;
    //         }

    //         if !is_start {
    //             is_start = true;
    //             start = std::time::Instant::now();
    //         }

    //         for wc in completed {
    //             match wc.opcode() {
    //                 ibverbs::ibv_wc_opcode::IBV_WC_SEND => {
    //                     req_cnt += 1;
    //                     if req_cnt >= 1000 {
    //                         exit_flag = true;
    //                         break;
    //                     }
    //                 }
    //                 _ => {
    //                     panic!("unexpected completion code {:?}, wc error: {:?}", 
    //                         wc.opcode(), 
    //                         wc.error()
    //                     );
    //                 },
    //             }
    //         }

    //         if exit_flag {
    //             break;
    //         }
    //     }

    //     // sleep for 10 seconds
    //     tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    //     let end = std::time::Instant::now();
    //     let elapsed = end - start;
    //     println!("elapsed: {:?}", elapsed);
    //     std::process::exit(0);
    // }); 

    // push_handle.await.unwrap();
    // poll_handle.await.unwrap();

    // sleep for 10 seconds 
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
}