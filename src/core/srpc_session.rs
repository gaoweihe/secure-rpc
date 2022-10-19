use tracing::{error};

#[allow(unused_imports)]
use crate::msg::srpc_msg::{RpcMsgPayload};
use crate::{core::srpc_dispatcher::RpcDispatcher, msg::srpc_msg::{RpcMsgHandle, RpcOnceMsg}}; 

#[derive(PartialEq, Debug)]
pub enum RpcSessionStatus {
    // The session is not connected to any server.
    Disconnected,
    // The session is connecting to a server.
    Connecting, 
    // The session is connected to a server.
    Connected,
    // The session is connected to a server and running logic.
    Running, 
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct RpcSession {
    status: RpcSessionStatus,
    // The server address.
    peer_id: u32, 
    peer_uri: std::string::String, 
    dispatcher: std::sync::Arc<RpcDispatcher>,
}

#[allow(dead_code)]
#[allow(unused_variables)]
impl RpcSession {
    pub fn new(
        peer_id: u32, 
        peer_uri: std::string::String, 
        dispatcher: std::sync::Arc<RpcDispatcher>
    ) -> RpcSession {
        RpcSession {
            status: RpcSessionStatus::Disconnected,
            peer_id: peer_id,
            peer_uri: peer_uri, 
            dispatcher: dispatcher,
        }
    }

    pub fn push_request(&mut self, msg: RpcOnceMsg) -> bool {
        if self.status != RpcSessionStatus::Connected {
            error!("The session is not connected to any server.");
            return false;
        }

        let mut msg_handle = RpcMsgHandle::default();
        msg_handle.set_msg(msg);

        // report to dispatcher 
        self.dispatcher.push_req(msg_handle);

        true
    }

    pub fn disconnect(&mut self) -> bool {
        if self.status != RpcSessionStatus::Connected {
            return false;
        }

        self.status = RpcSessionStatus::Disconnected;

        // invoke network to disconnect

        true
    }
}