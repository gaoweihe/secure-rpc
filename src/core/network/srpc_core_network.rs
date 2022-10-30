#[derive(Debug)]
pub struct RpcNetworkCore
{
    // session_id -> session
    conn_map: std::sync::Arc<std::sync::RwLock<
        std::collections::BTreeMap<
            u32, (Vec<Vec<u8>>, Vec<Vec<u8>>)
        >
    >>, 
    wr_cnt: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

unsafe impl Send for RpcNetworkCore {}
unsafe impl Sync for RpcNetworkCore {}

use ibverbs::{MemoryRegion};
use once_cell::sync::OnceCell;

use serde::Serialize;
use tracing::error;
#[allow(unused_imports)]
use tracing::{info, trace};

use crate::{core::{srpc_core::{RPC_DISPATCHER, get_mut_from_immut}, network::srpc_grpc::SrpcGrpcPreComm}, conf::conf::RPC_CONF};
pub static IBVERBS_QP_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u32, ibverbs::QueuePair>
    >>> = OnceCell::new(); // session_id -> queue pair 
pub static IBVERBS_PD: OnceCell<std::sync::Arc<
    ibverbs::ProtectionDomain>> = OnceCell::new();
pub static IBVERBS_CTX: OnceCell<std::sync::Arc<
    ibverbs::Context>> = OnceCell::new(); 
// pub static IBVERBS_PQP: OnceCell<std::sync::Arc<
//     ibverbs::PreparedQueuePair>> = OnceCell::new();
pub static IBVERBS_SQ: OnceCell<std::sync::Arc<
    ibverbs::CompletionQueue>> = OnceCell::new();
pub static IBVERBS_RQ: OnceCell<std::sync::Arc<
    ibverbs::CompletionQueue>> = OnceCell::new();
pub static IBVERBS_WRID_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u64, u32>
    >>> = OnceCell::new(); // wr_id -> session_id
// send memory region 
pub static IBVERBS_SMR_VEC: OnceCell<std::vec::Vec<
    ibverbs::MemoryRegion<u8>
    >> = OnceCell::new(); // mr 
pub static IBVERBS_SMR_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u64, u32>
    >>> = OnceCell::new(); // wr_id -> mr_index 
pub static IBVERBS_SMR_MAP_INV: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u32, u64>
    >>> = OnceCell::new(); // mr_index -> wr_id 
// receive memory region 
pub static IBVERBS_RMR_VEC: OnceCell<std::vec::Vec<
    ibverbs::MemoryRegion<u8>
    >> = OnceCell::new(); // mr 
pub static IBVERBS_RMR_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u64, u32>
    >>> = OnceCell::new(); // wr_id -> mr_index 
pub static IBVERBS_RMR_MAP_INV: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u32, u64>
    >>> = OnceCell::new(); // mr_index -> wr_id 

#[allow(unused_variables)]
impl RpcNetworkCore
{
    pub fn new_singleton() -> RpcNetworkCore
    {
        let net_core = RpcNetworkCore
        {
            conn_map: std::sync::Arc::new(
                std::sync::RwLock::new(
                    std::collections::BTreeMap::new()
                )),
            wr_cnt: std::sync::Arc::new(1.into()),
        };

        net_core.init();

        net_core
    }

    fn get_wr_id(&self) -> u64
    {
        let wr_cnt = self.wr_cnt.fetch_add(
            1, 
            std::sync::atomic::Ordering::SeqCst
        );
        wr_cnt
    }

    fn init(&self)
    {
        info!("RpcNetworkCore: init");

        let _result = IBVERBS_QP_MAP.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_WRID_MAP.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_SMR_MAP.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_SMR_MAP_INV.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_RMR_MAP.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_RMR_MAP_INV.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        self.init_infiniband();
        self.init_grpc();

    }

    fn init_grpc(&self)
    {
        info!("init_grpc");

        let listen_addr = "0.0.0.0:9000";
        let _handle = tokio::spawn(async move {
            let result = 
                SrpcGrpcPreComm::serve(listen_addr).await;
            match result {
                Ok(_) => {
                    info!("grpc server stopped");
                },
                Err(e) => {
                    info!("grpc server stopped with error: {:?}", e);
                },
            }
        });
    }

    fn init_infiniband(&self)
    {
        let conf = RPC_CONF.get().unwrap();

        let ctx = ibverbs::devices().unwrap()
            .iter().next().expect("no rdma device available")
            .open().unwrap();
        let _result = IBVERBS_CTX.set(
            std::sync::Arc::new(ctx)
        );

        let ctx = IBVERBS_CTX.get().unwrap();
        let sq = ctx.create_cq(64, 0).unwrap();
        let rq = ctx.create_cq(64, 1).unwrap();
        let pd = ctx.alloc_pd().unwrap();

        let _result = IBVERBS_SQ.set(
            std::sync::Arc::new(sq)
        );
        let _result = IBVERBS_RQ.set(
            std::sync::Arc::new(rq)
        );
        let _result = IBVERBS_PD.set(
            std::sync::Arc::new(pd)
        );

        // create memory regions 
        let pd = IBVERBS_PD.get().unwrap();
        let mut smr_vec = Vec::new();
        let smr_size = conf.loc_mr_size as usize;
        for _i in 0..1024
        {
            let smr = pd.allocate::<u8>(smr_size).unwrap();
            smr_vec.push(smr);
        }
        let _result = IBVERBS_SMR_VEC.set(smr_vec);

        let mut rmr_vec = Vec::new();
        let rmr_size = conf.loc_mr_size as usize;
        for _i in 0..1024
        {
            let rmr = pd.allocate::<u8>(rmr_size).unwrap();
            rmr_vec.push(rmr);
        }
        let _result = IBVERBS_RMR_VEC.set(rmr_vec); 

        // start polling work request queues 
        let sq = IBVERBS_SQ.get().unwrap();
        let rq = IBVERBS_RQ.get().unwrap();
        let _pollsq_handle = tokio::spawn(async move {
            Self::poll_cq(sq);
        });
        let _pollrq_handle = tokio::spawn(async move {
            Self::poll_cq(rq);
        }); 
    }

    fn poll_cq(cq: &std::sync::Arc<ibverbs::CompletionQueue>)
    {
        let mut completions = [ibverbs::ibv_wc::default(); 32];

        loop {
            let completed = cq.poll(&mut completions[..]).unwrap();
            if completed.is_empty() {
                continue;
            }
            for wc in completed {
                match wc.opcode() {
                    ibverbs::ibv_wc_opcode::IBV_WC_SEND => {
                        let wr_id = wc.wr_id();
                        trace!("IBV_WC_SEND wr_id={}", wr_id);
                        Self::release_occupied_smr(wr_id);
                    }
                    ibverbs::ibv_wc_opcode::IBV_WC_RECV => {
                        let wr_id = wc.wr_id();
                        trace!("IBV_WC_RECV wr_id={}", wr_id);
                        Self::on_recv(wr_id);
                        Self::release_occupied_rmr(wr_id);
                    }
                    _ => {
                        error!("unexpected completion: {:?}", wc.error());
                        // panic!("unexpected completion");
                    }
                }
            }
        }
    }

    fn get_vacant_smr(&self, wr_id: u64) -> Option<&mut ibverbs::MemoryRegion<u8>>
    {
        let mut mr_map = IBVERBS_SMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_SMR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        let mr_vec_len = IBVERBS_SMR_VEC.get().unwrap().len();
        for i in 0..mr_vec_len // mr_index 
        {
            let mr_index = i as u32;
            if !mr_map_inv.contains_key(&mr_index)
            {
                mr_map.insert(wr_id, mr_index);
                mr_map_inv.insert(mr_index, wr_id);

                let vacant_mr = IBVERBS_SMR_VEC.get().unwrap()
                    .get(i).unwrap();
                let vacant_mr_mut: &mut MemoryRegion<u8>;
                unsafe {
                    vacant_mr_mut = get_mut_from_immut(vacant_mr);
                }
                trace!("get_vacant_mr: wr_id = {}, mr_index = {}", wr_id, mr_index);
                return Some(vacant_mr_mut);
            }
        }
        None
    }

    fn get_vacant_rmr(&self, wr_id: u64) -> Option<&mut ibverbs::MemoryRegion<u8>>
    {
        let mut mr_map = IBVERBS_RMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_RMR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        let mr_vec_len = IBVERBS_RMR_VEC.get().unwrap().len();
        for i in 0..mr_vec_len // mr_index 
        {
            let mr_index = i as u32;
            if !mr_map_inv.contains_key(&mr_index)
            {
                mr_map.insert(wr_id, mr_index);
                mr_map_inv.insert(mr_index, wr_id);

                let vacant_mr = IBVERBS_RMR_VEC.get().unwrap()
                    .get(i).unwrap();
                let vacant_mr_mut: &mut MemoryRegion<u8>;
                unsafe {
                    vacant_mr_mut = get_mut_from_immut(vacant_mr);
                }
                trace!("get_vacant_mr: wr_id = {}, mr_index = {}", wr_id, mr_index);
                return Some(vacant_mr_mut);
            }
        }
        None
    }

    fn release_occupied_smr(wr_id: u64)
    {
        let mut mr_map = IBVERBS_SMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_SMR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        if mr_map.contains_key(&wr_id)
        {
            let mr_index = mr_map.remove(&wr_id).unwrap();
            mr_map_inv.remove(&mr_index);
        }
    }

    fn release_occupied_rmr(wr_id: u64)
    {
        let mut mr_map = IBVERBS_RMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_RMR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        if mr_map.contains_key(&wr_id)
        {
            let mr_index = mr_map.remove(&wr_id).unwrap();
            mr_map_inv.remove(&mr_index);
        }
    }

    pub fn send_to(&self, session_id: u32, bin: &[u8])
    {
        let conf = RPC_CONF.get().unwrap();
        let mr_size = conf.loc_mr_size as usize;

        let data_len = bin.len();
        trace!("send_to: session_id = {}, data_len = {}", session_id, data_len);
        trace!("send_to: raw_msg_len = {:?}", u16::from_be_bytes([
            bin[mr_size - 2], 
            bin[mr_size - 1]]
        ));

        // invoke network to send request
        let qp_map = IBVERBS_QP_MAP.get().unwrap();
        let mut qp_map = qp_map.lock().unwrap();
        let qp = qp_map.get_mut(&session_id).unwrap();

        let wr_id = self.get_wr_id();
        trace!("post_receive trying to get_vacant_mr: wr_id = {}", wr_id);
        let mut mr_recv = self.get_vacant_rmr(wr_id).unwrap(); 

        IBVERBS_WRID_MAP.get().unwrap()
            .lock().unwrap()
            .insert(wr_id, session_id);
        unsafe { 
            qp.post_receive(
                &mut mr_recv, 
                .., 
                wr_id
            ) 
        }.unwrap();
        trace!("post_receive: wr_id = {}", wr_id);

        let wr_id = self.get_wr_id();
        trace!("post_send trying to get_vacant_mr: wr_id = {}", wr_id);
        let mut mr_send = self.get_vacant_smr(wr_id).unwrap(); 
        mr_send.clone_from_slice(bin);
        IBVERBS_WRID_MAP.get().unwrap()
            .lock().unwrap()
            .insert(wr_id, session_id);
        unsafe { 
            qp.post_send(
                &mut mr_send, 
                .., 
                wr_id
            )
        }.unwrap();
        trace!("post_send: wr_id = {}", wr_id);

    }

    pub fn on_recv(wr_id: u64)
    {
        let conf = RPC_CONF.get().unwrap();
        let mr_size = conf.loc_mr_size as usize;

        let session_id = Self::get_session_id_by_wr_id(wr_id);
        let mr_index = Self::get_rmr_index_by_wr_id(wr_id).unwrap();

        trace!("on_recv: wr_id = {}, mr_index = {}", wr_id, mr_index);

        let mr = IBVERBS_RMR_VEC.get().unwrap()
            .get(mr_index as usize).unwrap();
        let mr_mut = unsafe { get_mut_from_immut(mr) };
        let raw_msg_len = u16::from_be_bytes([
            mr_mut[mr_size - 2], 
            mr_mut[mr_size - 1]
            ]);
        trace!("on_recv: raw_msg_len = {:?}", raw_msg_len);
        
        let mut mr_vec = vec![0; mr_size];
        let mr_slice = mr_vec.as_mut_slice();
        trace!("mr_slice.len() = {}", mr_slice.len());
        mr_mut.swap_with_slice(mr_slice);

        RPC_DISPATCHER.get().unwrap().on_recv_msg(session_id, mr_slice);
    }

    fn get_smr_index_by_wr_id(wr_id: u64) -> Option<u32>
    {
        let mr_map = IBVERBS_SMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        if mr_map.contains_key(&wr_id)
        {
            Some(mr_map.get(&wr_id).unwrap().clone())
        }
        else
        {
            None
        }
    }

    fn get_rmr_index_by_wr_id(wr_id: u64) -> Option<u32>
    {
        let mr_map = IBVERBS_RMR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        if mr_map.contains_key(&wr_id)
        {
            Some(mr_map.get(&wr_id).unwrap().clone())
        }
        else
        {
            None
        }
    }

    pub fn on_send(wr_id: u64)
    {
        let smr_index = Self::get_smr_index_by_wr_id(wr_id).unwrap();

    }

    // FIXME: change return value to Result<> type 
    pub async fn connect_to(&self, session_id: u32, peer_uri: &str) -> bool
    {
        trace!("connect_to: session_id = {}, peer_uri = {}", session_id, peer_uri);
        self.conn_map.write().unwrap()
            .insert(session_id, (Vec::new(), Vec::new()));
        
        // connect 
        let pd = IBVERBS_PD.get().unwrap();
        let sq = IBVERBS_SQ.get().unwrap();
        let rq = IBVERBS_RQ.get().unwrap();
        let qp_builder = pd.create_qp(
            &sq, 
            64, 
            &rq, 
            64, 
            ibverbs::ibv_qp_type::IBV_QPT_RC
        ).build().unwrap();
        let loc_endpoint = 
            qp_builder.endpoint(); // local endpoint 

        let mut serializer = 
            flexbuffers::FlexbufferSerializer::new(); 
        loc_endpoint.serialize(&mut serializer).unwrap();
        let endpoint_bin = serializer.view();
        let endpoint_bin_vec = endpoint_bin.to_vec();

        let rmt_endpoint = 
            SrpcGrpcPreComm::get_endpoint(
                &loc_endpoint, 
                peer_uri
            ).await.unwrap(); 

        let qp = qp_builder.handshake(rmt_endpoint).unwrap(); 
        info!("qp_map insert session_id: {}", session_id);
        IBVERBS_QP_MAP.get().unwrap().lock().unwrap()
            .insert(session_id, qp);
        
        true
    }

    pub fn disconnect(&mut self, conn_id: u32)
    {
        // invoke network module to disconnect
    }
}

impl RpcNetworkCore
{
    fn get_session_id_by_wr_id(wr_id: u64) -> u32
    {
        let wrid_map = IBVERBS_WRID_MAP.get().unwrap();
        let wrid_map_lock = wrid_map.lock().unwrap();
        let session_id = wrid_map_lock.get(&wr_id).unwrap();
        *session_id
    }
}
