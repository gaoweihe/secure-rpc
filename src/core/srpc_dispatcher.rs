use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

#[allow(unused_imports)]
use tracing::{info, trace};

use crate::conf::conf::RPC_CONF;
use crate::msg::srpc_msg::{RpcMsgHandle, RpcOnceMsg};
use crate::core::srpc_core::RPC_CORE;
use crate::core::network::srpc_core_network::RpcNetworkCore; 
use crate::core::srpc_session::RpcSession;

use super::srpc_core::RPC_DISPATCHER;
use super::srpc_session::SESSION_COUNTER; 

#[allow(dead_code)]
#[derive(Debug)]
pub struct RpcDispatcher
{
    send_req_queue: std::sync::Arc<std::sync::RwLock<
        VecDeque<RpcMsgHandle>
        >>,
    send_resp_queue: std::sync::Arc<std::sync::RwLock<
        VecDeque<RpcMsgHandle>
        >>, 
    recv_req_queue: std::sync::Arc<std::sync::RwLock<
        VecDeque<RpcMsgHandle>
        >>, 
    recv_resp_queue: std::sync::Arc<std::sync::RwLock<
        VecDeque<RpcMsgHandle>
        >>, 
    network: RpcNetworkCore, 
    session_map: std::sync::Arc<std::sync::RwLock<
        std::collections::BTreeMap<u32, RpcSession>
        >>,
    peer_map: std::sync::Arc<std::sync::RwLock<
        std::collections::BTreeMap<u32, u32>
        >>, // peer_id -> session_id 
    session_counter: std::sync::atomic::AtomicU32,
}

unsafe impl Send for RpcDispatcher {}
unsafe impl Sync for RpcDispatcher {}

#[allow(dead_code)]
impl RpcDispatcher
{
    pub fn new_arc() -> std::sync::Arc<RpcDispatcher>
    {
        let dispatcher = RpcDispatcher {
            send_req_queue: std::sync::Arc::new(std::sync::RwLock::new(
                VecDeque::new()
                )),
            send_resp_queue: std::sync::Arc::new(std::sync::RwLock::new(
                VecDeque::new()
                )),
            recv_req_queue: std::sync::Arc::new(std::sync::RwLock::new(
                VecDeque::new()
                )),
            recv_resp_queue: std::sync::Arc::new(std::sync::RwLock::new(
                VecDeque::new()
                )),
            network: crate::core::network::srpc_core_network::RpcNetworkCore::new_singleton(), 
            session_map: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::BTreeMap::new()
            )),
            session_counter: std::sync::atomic::AtomicU32::new(0),
            peer_map: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::BTreeMap::new()
            )),
        }; 

        let dispatcher = std::sync::Arc::new(dispatcher);

        SESSION_COUNTER.set(
            std::sync::Arc::new(
                std::sync::atomic::AtomicU32::new(0)
            )
        ).unwrap();

        dispatcher
    }

    pub fn set_network(&mut self, network: crate::core::network::srpc_core_network::RpcNetworkCore)
    {
        self.network = network;
    }

    pub async fn connect_to(&self, peer_id: u32, peer_uri: &str) -> Result<u32, Box<dyn std::error::Error>>
    {
        let session_id = RpcSession::get_session_id();

        let _ = self.network.connect_to(session_id, peer_uri).await;
        let session = RpcSession::new(
            peer_id, 
            peer_uri.to_string(), 
            RPC_DISPATCHER.get().unwrap().clone()
        );
        self.session_map.write().unwrap()
            .insert(session_id, session);
        self.peer_map.write().unwrap()
            .insert(peer_id, session_id); 

        trace!("connected to peer {}:{} with session {}", 
            peer_id, peer_uri, session_id);

        // FIXME: return error on failure 
        Ok(session_id)
    }

    pub fn on_recv_msg(&self, session_id: u32, bin: &[u8])
    {  
        let conf = RPC_CONF.get().unwrap();
        let mr_size = conf.loc_mr_size as usize;

        let raw_len = u16::from_be_bytes([
            bin[mr_size - 2], 
            bin[mr_size - 1]
        ]) as usize;
        trace!("on_recv_msg: raw_len {}", raw_len);
        let raw_msg = &bin[..raw_len];
        trace!("on_recv_msg: raw_msg.len() {}", raw_msg.len());
        let reader = 
            flexbuffers::Reader::get_root(raw_msg).unwrap();
        let msg = 
            RpcOnceMsg::deserialize(reader).unwrap();

        let mut msg_handle = RpcMsgHandle::default();
        msg_handle.set_msg(msg);

        match msg_handle.msg_type
        {
            crate::msg::srpc_msg::RpcMsgType::Request => 
            {
                self.on_recv_req(session_id, msg_handle);
            },
            crate::msg::srpc_msg::RpcMsgType::Response => 
            {
                self.on_recv_resp(session_id, msg_handle);
            },
            _ => 
            {
                trace!("on_recv_msg: unknown msg type");
            },
        }

    }

    pub fn on_recv_req(&self, _peer_id: u32, req: RpcMsgHandle)
    {
        let mut queue_lock = self.recv_req_queue.write().unwrap();
        queue_lock.push_back(req);
        trace!("on_recv_req: {}", queue_lock.len());
    }

    pub fn on_recv_resp(&self, _peer_id: u32, resp: RpcMsgHandle)
    {
        let mut queue_lock = self.recv_resp_queue.write().unwrap();
        queue_lock.push_back(resp);
        trace!("on_recv_resp: {}", queue_lock.len());
    }

    pub fn push_req(&self, req: RpcMsgHandle)
    {
        let mut queue_lock = self.send_req_queue.write().unwrap();
        queue_lock.push_back(req);

        trace!("push_req: {}", queue_lock.len());
    }

    pub fn push_resp(&self, resp: RpcMsgHandle)
    {
        let mut queue_lock = self.send_resp_queue.write().unwrap();
        queue_lock.push_back(resp);
    }

    fn check_recv_req(&self)
    {
        let mut queue_lock = self.recv_req_queue.write().unwrap();
        let len = queue_lock.len();
        for _ in 0..len {
            let msg = queue_lock.pop_front().unwrap();
            trace!("check_recv_req: msg = {:?}", msg);
            let req_type = msg.msg.req_type;

            let core = RPC_CORE.get().unwrap();
            let cb = core.get_cb_by_reqtype(req_type).unwrap();
            (*cb)(msg);
        }
    }

    fn check_send_req(&self)
    {
        let conf = RPC_CONF.get().unwrap();
        let mr_size = conf.loc_mr_size as usize;

        let mut queue_lock = self.send_req_queue.write().unwrap();
        let len = queue_lock.len();
        for _ in 0..len {
            let msg_handle = queue_lock.pop_front().unwrap();
            trace!("check_send_req: msg = {:?}", msg_handle);

            let rpc_msg = msg_handle.msg;

            // ensure message buffer is of equal length to 
            // pre-allocated RDMA memory region 
            let mut serializer = 
                flexbuffers::FlexbufferSerializer::new();
            rpc_msg.serialize(&mut serializer).unwrap();
            let msg_bin = serializer.view();
            let raw_msg_bin_len = msg_bin.len() as u16;
            trace!("check_send_req: flexbuff msg_bin len= {:?}", raw_msg_bin_len);
            let mut msg_bin = msg_bin.to_vec();

            msg_bin.resize(mr_size, 0);
            let msg_bin = msg_bin.as_mut_slice();
            trace!("check_send_req: resize msg_bin len= {:?}", msg_bin.len());
            msg_bin[mr_size - 2] = raw_msg_bin_len.to_be_bytes()[0];
            msg_bin[mr_size - 1] = raw_msg_bin_len.to_be_bytes()[1];

            let session_id = self.get_session_id_by_peer_id(
                msg_handle.peer_id).unwrap();
            self.network.send_to(session_id, msg_bin);
        }
    }

    fn check_recv_resp(&self)
    {

    }

    fn check_send_resp(&self)
    {

    }

    pub fn run_loop_once(&self)
    {
        self.check_recv_req();
        self.check_send_req();
        self.check_recv_resp();
        self.check_send_resp();
    }
}

// Utility functions for RpcDispatcher 
impl RpcDispatcher {
    fn get_session_id_by_peer_id(&self, peer_id: u32) -> Option<u32>
    {
        let peer_map = self.peer_map.read().unwrap();
        let session_id = peer_map.get(&peer_id);
        match session_id
        {
            Some(id) => Some(*id),
            None => None,
        }
    }
}