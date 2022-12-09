include!("common.rs");

#[allow(unused_imports)]
use tracing::info;

use serde::Deserialize;
use serde::Serialize;

use precomm_grpc::*;
pub mod precomm_grpc {
    tonic::include_proto!("pre_comm");
}
use precomm_grpc::pre_comm_service_server::*; 

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

// Communication through legacy TCP sockets functionality 
// before RDMA connection is established. 
#[derive(Debug)]
pub struct SrpcGrpcPreComm { } 

#[allow(unused_variables)]
#[tonic::async_trait]
impl PreCommService for SrpcGrpcPreComm {
    async fn get_endpoint(
        &self,
        request: tonic::Request<precomm_grpc::GetEndpointRequest>,
    ) -> Result<tonic::Response<GetEndpointResponse>, tonic::Status> {
        let request = request.into_inner();
        let clt_endpoint_bin = request.src_endpoint;
        REMOTE_ENDPOINT.set(
            std::sync::Arc::new(
                clt_endpoint_bin.clone()
            )
        ).unwrap();
        let clt_endpoint_slice = clt_endpoint_bin.as_slice();
        let reader = 
            flexbuffers::Reader::get_root(clt_endpoint_slice).unwrap(); 
        let src_endpoint = 
            ibverbs::QueuePairEndpoint::deserialize(reader).unwrap();

        // serialize designated endpoint 
        let loc_endpoint_bin_vec = 
            LOCAL_ENDPOINT.get().unwrap();

        let response = GetEndpointResponse {
            endpoint: loc_endpoint_bin_vec.to_vec()
        };

        // sleep for 1 second to wait 
        // for polling thread to start 
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Ok(tonic::Response::new(response))
    }
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

    // prepare static domains 
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
        ctx.create_cq(1024, 0).unwrap()
    ).unwrap();
    let rq = RECEIVE_QUEUE.get().unwrap();

    SEND_QUEUE.set(
        ctx.create_cq(8, 0).unwrap()
    ).unwrap();
    let sq = SEND_QUEUE.get().unwrap();

    // prepared queue pair 
    let qp_builder = pd.create_qp(
        &sq, 
        8, 
        &rq, 
        1024, 
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

    // gRPC start listening 
    let addr = format!("0.0.0.0:50051").parse().unwrap(); 
    info!("SrpcGrpcPreComm: trying to serve on {:?}", addr);
    let grpc_handle = tokio::spawn({
        tonic::transport::Server::builder()
            .add_service(PreCommServiceServer::new(SrpcGrpcPreComm {}))
            .serve(addr)
    });

    // start pushing receive requests 
    let push_handle = tokio::spawn(async move {
        let mut mr = pd.allocate::<u8>(1048576).unwrap();

        let mut wr_id = 10000000;

        // poll for remote endpoint 
        info!("waiting for remote endpoint");
        let rmt_ep: ibverbs::QueuePairEndpoint;
        loop {
            let ep_res = REMOTE_ENDPOINT.get();
            match ep_res {
                Some(ep_bin) => {
                    rmt_ep = deserialize_endpoint(ep_bin.to_vec());
                    info!("remote endpoint: {:?}", rmt_ep);
                    break;
                },
                None => {
                    // tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        // handshake with remote endpoint 
        let mut qp = qp_builder.handshake(rmt_ep).unwrap();

        let result = unsafe { 
            qp.post_receive(&mut mr, .., 2)
        };
        match result {
            Ok(_) => {
                wr_id += 1;
                // info!("post_send: OK: wr_id = {}", wr_id);
            },
            Err(e) => {
                info!("post_send: Err: {:?}", e);
            }
        }

        let result = unsafe { 
            qp.post_send(&mut mr, .., 1)
        };
        match result {
            Ok(_) => {
                wr_id += 1;
                // info!("post_send: OK: wr_id = {}", wr_id);
            },
            Err(e) => {
                info!("post_send: Err: {:?}", e);
            }
        }

        loop {
            let result = unsafe { 
                qp.post_receive(&mut mr, .., wr_id)
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

    // start polling receive requests
    let rq_poll = rq.clone();
    let poll_handle = tokio::spawn(async move {
        let mut completions = [ibverbs::ibv_wc::default(); 100];

        loop {
            let completed = rq_poll.poll(&mut completions[..]).unwrap();
            if completed.is_empty() {
                continue;
            }
            for wc in completed {
                match wc.opcode() {
                    ibverbs::ibv_wc_opcode::IBV_WC_RECV => {
                    }
                    _ => {
                        panic!("unexpected completion code {:?}, wc error: {:?}", 
                            wc.opcode(), 
                            wc.error()
                        );
                    },
                }
            }
        }
    }); 

    push_handle.await.unwrap();
    poll_handle.await.unwrap();
    grpc_handle.await.unwrap().unwrap();
}
