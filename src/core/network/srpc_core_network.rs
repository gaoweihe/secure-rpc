#[derive(Debug)]
pub struct RpcNetworkCore
{
    // session_id -> session
    conn_map: std::sync::Arc<std::sync::RwLock<std::collections::BTreeMap<
        u32, (Vec<Vec<u8>>, Vec<Vec<u8>>)
    >>>, 
    wr_cnt: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

unsafe impl Send for RpcNetworkCore {}
unsafe impl Sync for RpcNetworkCore {}

use ibverbs::MemoryRegion;
use once_cell::sync::OnceCell;

#[allow(unused_imports)]
use tracing::{info, trace};

use crate::core::srpc_core::{RPC_DISPATCHER, get_mut_from_immut};
static IBVERBS_QP_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u32, ibverbs::QueuePair>
    >>> = OnceCell::new();
static IBVERBS_PD: OnceCell<std::sync::Arc<
    ibverbs::ProtectionDomain>> = OnceCell::new();
static IBVERBS_CTX: OnceCell<std::sync::Arc<
    ibverbs::Context>> = OnceCell::new(); 
static IBVERBS_CQ: OnceCell<std::sync::Arc<
    ibverbs::CompletionQueue>> = OnceCell::new();
static IBVERBS_WRID_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u64, u32>
    >>> = OnceCell::new(); // wr_id -> session_id
static IBVERBS_MR_VEC: OnceCell<std::vec::Vec<
    ibverbs::MemoryRegion<u8>
    >> = OnceCell::new(); // mr 
static IBVERBS_MR_MAP: OnceCell<std::sync::Arc<std::sync::Mutex<
    std::collections::BTreeMap<u64, u32>
    >>> = OnceCell::new(); // wr_id -> mr_index 
static IBVERBS_MR_MAP_INV: OnceCell<std::sync::Arc<std::sync::Mutex<
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
        let _result = IBVERBS_MR_MAP.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        let _result = IBVERBS_MR_MAP_INV.set(
            std::sync::Arc::new(
            std::sync::Mutex::new(
                std::collections::BTreeMap::new()
            )));
        self.init_infiniband();
    }

    fn init_infiniband(&self)
    {
        let ctx = ibverbs::devices().unwrap()
            .iter().next().expect("no rdma device available")
            .open().unwrap();
        let _result = IBVERBS_CTX.set(
            std::sync::Arc::new(ctx)
        );

        let ctx = IBVERBS_CTX.get().unwrap();
        let cq = ctx.create_cq(16, 0).unwrap();
        let pd = ctx.alloc_pd().unwrap();

        let _result = IBVERBS_CQ.set(
            std::sync::Arc::new(cq)
        );
        let _result = IBVERBS_PD.set(
            std::sync::Arc::new(pd)
        );

        let pd = IBVERBS_PD.get().unwrap();
        let mut mr_vec = Vec::new();
        for _i in 0..16
        {
            let mr = pd.allocate::<u8>(2048).unwrap();
            mr_vec.push(mr);
        }
        let _result = IBVERBS_MR_VEC.set(mr_vec);

        tokio::spawn(async move {
            let cq = IBVERBS_CQ.get().unwrap();
            let pd = IBVERBS_PD.get().unwrap();
            let qp_builder = pd.create_qp(
                &cq, 
                &cq, 
                ibverbs::ibv_qp_type::IBV_QPT_RC
            ).build().unwrap();

            let mut completions = [ibverbs::ibv_wc::default(); 16];
            loop {
                let completed = cq.poll(&mut completions[..]).unwrap();
                if completed.is_empty() {
                    continue;
                }
                assert!(completed.len() <= 2);
                for wc in completed {
                    match wc.opcode() {
                        ibverbs::ibv_wc_opcode::IBV_WC_SEND => {
                            let wr_id = wc.wr_id();
                            trace!("IBV_WC_SEND wr_id={}", wr_id);
                            Self::release_occupied_mr(wr_id);
                        }
                        ibverbs::ibv_wc_opcode::IBV_WC_RECV => {
                            let wr_id = wc.wr_id();
                            trace!("IBV_WC_RECV wr_id={}", wr_id);
                            Self::on_recv(wr_id);
                            Self::release_occupied_mr(wr_id);
                        }
                        _ => {
                            trace!("wc.opcode() = {:?}", wc.opcode());
                            panic!("unexpected completion");
                        }
                    }
                }
            }
        });
    }

    fn get_vacant_mr(&self, wr_id: u64) -> Option<&mut ibverbs::MemoryRegion<u8>>
    {
        let mut mr_map = IBVERBS_MR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_MR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        let mr_vec_len = IBVERBS_MR_VEC.get().unwrap().len();
        for i in 0..mr_vec_len // mr_index 
        {
            let mr_index = i as u32;
            if !mr_map_inv.contains_key(&mr_index)
            {
                mr_map.insert(wr_id, mr_index);
                mr_map_inv.insert(mr_index, wr_id);

                let vacant_mr = IBVERBS_MR_VEC.get().unwrap()
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

    fn release_occupied_mr(wr_id: u64)
    {
        let mut mr_map = IBVERBS_MR_MAP.get().unwrap()
            .lock().unwrap(); // wr_id -> mr_index
        let mut mr_map_inv = IBVERBS_MR_MAP_INV.get().unwrap()
            .lock().unwrap(); // mr_index -> wr_id
        if mr_map.contains_key(&wr_id)
        {
            let mr_index = mr_map.remove(&wr_id).unwrap();
            mr_map_inv.remove(&mr_index);
        }
    }

    pub fn check_conn_map(&mut self)
    {
        let mut rq: Vec<(u32, Vec<u8>)> = Vec::new();

        {
            let mut conn_map = self.conn_map.write().unwrap();
            for (session_id, (send_queue, recv_queue)) in conn_map.iter_mut()
            {
                let recv_queue_len = recv_queue.len();
                for i in 0..recv_queue_len
                {
                    let msg = recv_queue.get(i).unwrap();
                    rq.push((session_id.clone(), msg.clone()));
                }
                recv_queue.clear();
            }
        }

        // for (session_id, msg) in rq
        // {
        //     self.on_recv(session_id, msg.as_slice());
        // }
    }

    pub fn send_to(&self, session_id: u32, bin: &[u8])
    {
        let data_len = bin.len();
        trace!("send_to: session_id = {}, data_len = {}", session_id, data_len);
        trace!("send_to: raw_msg_len = {:?}", u16::from_be_bytes([bin[2046], bin[2047]]));
        trace!("send_to: raw_msg = {:?}", &bin[..]);

        // invoke network to send request
        let qp_map = IBVERBS_QP_MAP.get().unwrap();
        let mut qp_map = qp_map.lock().unwrap();
        let qp = qp_map.get_mut(&session_id).unwrap();

        let wr_id = self.get_wr_id();
        let mut mr_recv = self.get_vacant_mr(wr_id).unwrap(); 

        IBVERBS_WRID_MAP.get().unwrap()
            .lock().unwrap()
            .insert(wr_id, session_id);
        unsafe { 
            qp.post_receive(&mut mr_recv, std::ops::Range{ start: 0, end: 2048}, wr_id) 
        }.unwrap();
        trace!("post_receive: wr_id = {}", wr_id);

        let wr_id = self.get_wr_id();
        let mut mr_send = self.get_vacant_mr(wr_id).unwrap(); 
        mr_send.clone_from_slice(bin);
        trace!("{:?} {:?}", mr_send[0], mr_send[1]);
        IBVERBS_WRID_MAP.get().unwrap()
            .lock().unwrap()
            .insert(wr_id, session_id);
        unsafe { 
            qp.post_send(&mut mr_send, std::ops::Range{ start: 0, end: 2048}, wr_id) 
        }.unwrap();
        trace!("post_send: wr_id = {}", wr_id);

    }

    pub fn on_recv(wr_id: u64)
    {
        let session_id = Self::get_session_id_by_wr_id(wr_id);
        let mr_index = Self::get_mr_index_by_wr_id(wr_id).unwrap();

        trace!("on_recv: wr_id = {}, mr_index = {}", wr_id, mr_index);

        let mr = IBVERBS_MR_VEC.get().unwrap()
            .get(mr_index as usize).unwrap();
        let mr_mut = unsafe { get_mut_from_immut(mr) };
        let raw_msg_len = u16::from_be_bytes([mr_mut[2046], mr_mut[2047]]);
        trace!("on_recv: raw_msg_len = {:?}", raw_msg_len);
        
        let mut mr_vec = vec![0; 2048];
        let mr_slice = mr_vec.as_mut_slice();
        trace!("mr_slice.len() = {}", mr_slice.len());
        mr_mut.swap_with_slice(mr_slice);
        trace!("mr_slice: {:?}", mr_slice);

        RPC_DISPATCHER.get().unwrap().on_recv_msg(session_id, mr_slice);
    }

    fn get_mr_index_by_wr_id(wr_id: u64) -> Option<u32>
    {
        let mr_map = IBVERBS_MR_MAP.get().unwrap()
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

    pub fn on_send(&mut self, session_id: u32)
    {
        

    }

    pub fn connect_to(&self, session_id: u32, peer_uri: &str) -> bool
    {
        self.conn_map.write().unwrap()
            .insert(session_id, (Vec::new(), Vec::new()));
        
        // connect 
        let pd = IBVERBS_PD.get().unwrap();
        let cq = IBVERBS_CQ.get().unwrap();
        let qp_builder = pd.create_qp(
            &cq, // loopback cq 
            &cq, // loopback cq 
            ibverbs::ibv_qp_type::IBV_QPT_RC
        ).build().unwrap();
        let endpoint = qp_builder.endpoint(); // loopback endpoint 
        info!("local endpoint = {:?}", endpoint);
        let qp = qp_builder.handshake(endpoint).unwrap(); 
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
