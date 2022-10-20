use precomm_grpc::*;
pub mod precomm_grpc {
    tonic::include_proto!("pre_comm");
}
use precomm_grpc::pre_comm_service_server::*;
use precomm_grpc::pre_comm_service_client::*;

use serde::Deserialize;
use serde::Serialize;
use tracing::info;
use tracing::trace;
use tracing::{error};

use super::srpc_core_network::IBVERBS_CQ;
use super::srpc_core_network::IBVERBS_PD;

// Communication through legacy TCP sockets functionality 
// before RDMA connection is established. 
#[derive(Debug)]
pub struct SrpcGrpcPreComm { } 

#[tonic::async_trait]
impl PreCommService for SrpcGrpcPreComm {
    async fn get_endpoint(
        &self,
        request: tonic::Request<GetEndpointRequest>,
    ) -> Result<tonic::Response<GetEndpointResponse>, tonic::Status> {
        let request = request.into_inner();

        // serialize designated endpoint 
        let pd = IBVERBS_PD.get().unwrap();
        let cq = IBVERBS_CQ.get().unwrap();
        let qp_builder = pd.create_qp(
            &cq, 
            &cq, 
            ibverbs::ibv_qp_type::IBV_QPT_RC
        ).build().unwrap();
        let endpoint = qp_builder.endpoint();
        
        let mut serializer = 
            flexbuffers::FlexbufferSerializer::new();
        endpoint.serialize(&mut serializer).unwrap();
        let endpoint_bin = serializer.view();
        let endpoint_bin_vec = endpoint_bin.to_vec();

        let response = GetEndpointResponse {
            endpoint: endpoint_bin_vec
        };

        Ok(tonic::Response::new(response))
    }
}

impl SrpcGrpcPreComm {
    pub async fn serve(
        loc_uri: &str
    ) -> Result<(), tonic::transport::Error>
    {
        let addr = format!("{}", loc_uri).parse().unwrap(); 
        info!("SrpcGrpcPreComm: trying to serve on {:?}", addr);
        let result = tonic::transport::Server::builder()
            .add_service(PreCommServiceServer::new(SrpcGrpcPreComm {}))
            .serve(addr)
            .await;
        
        result
    }

    async fn connect_to(peer_uri: &str) 
    -> PreCommServiceClient<tonic::transport::Channel> {
        let peer_uri = peer_uri.to_string();

        loop {
            let conn_result = 
                PreCommServiceClient::connect(
                    peer_uri.clone()
                ).await; 
            match conn_result {
                Ok(conn) => {
                    return conn;
                },
                Err(e) => {
                    error!("gRPC failed to connect to server: {:?}", e);
                }
            }

            // sleep for 1 second and retry 
            tokio::time::sleep(
                std::time::Duration::from_secs(1)
            ).await;
        }
    }

    pub async fn get_endpoint(
        loc_endpoint: &ibverbs::QueuePairEndpoint, 
        peer_uri: &str
    ) -> Option<ibverbs::QueuePairEndpoint> {
        let mut conn_handle = 
            Self::connect_to(peer_uri).await;

        // serialize local endpoint 
        let mut serializer = 
            flexbuffers::FlexbufferSerializer::new(); 
        loc_endpoint.serialize(&mut serializer).unwrap();
        let endpoint_bin = serializer.view();
        let endpoint_bin_vec = endpoint_bin.to_vec();

        let request = GetEndpointRequest {
            src_endpoint: endpoint_bin_vec,
        };

        let result = 
            conn_handle.get_endpoint(
                request
            ).await; 
        
        match result {
            Ok(response) => {
                let response = response.into_inner();
                trace!("gRPC response: {:?}", response);

                let endpoint_bin = response.endpoint.as_slice();
                
                let reader = 
                    flexbuffers::Reader::get_root(
                        endpoint_bin
                    ).unwrap();
                let endpoint = 
                    ibverbs::QueuePairEndpoint::deserialize(
                        reader
                    ).unwrap();

                return Some(endpoint);
            },
            Err(e) => {
                error!("gRPC failed to get endpoint: {:?}", e);
                
                return None;
            }
        }
    }
}