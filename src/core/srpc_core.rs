use crate::msg::srpc_msg::{RpcMsgHandle};

use std::boxed::Box;
// type CallBackBox = Box<dyn Fn(RpcMsgHandle)>;

pub trait CallBack: Fn(RpcMsgHandle) { }
impl<F> CallBack for F where F: Fn(RpcMsgHandle) { }
impl std::fmt::Debug for dyn CallBack {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dyn CallBack")
    }
}
pub type CallBackBox = Box<dyn CallBack>;

use once_cell::sync::OnceCell;
use tracing::{info, trace};
pub static RPC_CORE: once_cell::sync::OnceCell<
    std::sync::Arc<RpcCore>
    > = OnceCell::new();
pub static RPC_DISPATCHER: once_cell::sync::OnceCell<
    std::sync::Arc<RpcDispatcher>
    > = OnceCell::new();

use super::{srpc_dispatcher::RpcDispatcher};

#[derive(PartialEq, Debug)]
pub enum RpcCoreStatus {
    // The RPC core is not running.
    Stopped,
    // The RPC core is running.
    Running,
    // The RPC core is shutting down.
    ShuttingDown,
}


#[allow(dead_code)]
#[derive(Debug)]
pub struct RpcCore {
    // The status of the RPC core.
    status: RpcCoreStatus,
    loc_id: u32, // local RPC ID
    loc_uri: std::string::String, // local URI 
    cnt_lwt: u64, // number of legacy worker threads 
    cnt_ewt: u64, // number of enclave worker threads 
    // sessions: std::vec::Vec<RpcSession>, 
    nn_id: Option<u64>, // numa node id (optional) 
    cb_map: std::collections::BTreeMap<
        u8, CallBackBox
        >, // req_type -> cb_box  

    // runtime section 
    req_counter: std::sync::atomic::AtomicU64, 
    runtime_lock: std::sync::Mutex<()>, 

    // runtime functionalities 
    pub dispatcher: std::sync::Arc<RpcDispatcher>,
}

// impl Send 
unsafe impl Send for RpcCore {}
// impl Sync
unsafe impl Sync for RpcCore {}


impl<'cb> RpcCore {
    // Creates a default RPC core.
    pub fn new() -> std::sync::Arc<Self> {
        let dispatcher = RpcDispatcher::new_arc();
        let core = Self {
            status: RpcCoreStatus::Stopped,
            loc_id: 0,
            loc_uri: std::string::String::new(),
            cnt_lwt: 0,
            cnt_ewt: 0,
            // sessions: std::Vec::new(),
            nn_id: None,
            cb_map: std::collections::BTreeMap::new(),
            req_counter: 0.into(),
            runtime_lock: std::sync::Mutex::new(()),
            dispatcher: dispatcher.clone(),
        };

        RPC_DISPATCHER.set(dispatcher).unwrap();

        let core = std::sync::Arc::new(core);
        RPC_CORE.set(core.clone()).unwrap();

        core
    }

    // Starts the RPC core.
    pub fn start(&self) -> Result<(), std::string::String> {

        let _unrefed_lock = self.runtime_lock.lock().unwrap();

        unsafe {
            let core_mut = get_mut_from_immut(self);

            // Check the status.
            if core_mut.status != RpcCoreStatus::Stopped {
                return Err("The RPC core is already running.".to_string());
            }

            
            // Set the status.
            core_mut.status = RpcCoreStatus::Running;

            info!("RPC core started.");
        }

        Ok(())
    }

    // Stop the RPC core.
    pub fn stop(&self) -> Result<(), std::string::String> {

        let _unrefed_lock = self.runtime_lock.lock().unwrap();

        unsafe {
            let core_mut = get_mut_from_immut(self);

            // Check the status.
            if self.status != RpcCoreStatus::Running {
                return Err("The RPC core is not running.".to_string());
            }

            // Set the status.
            core_mut.status = RpcCoreStatus::ShuttingDown;

            // Shutdown the RPC core.

            // Set the status.
            core_mut.status = RpcCoreStatus::Stopped;
        }

        Ok(())
    }

    // Registers a legacy callback function. 
    pub fn reg_legacy_cb(
        &self, 
        cb_index: u8, 
        cb: CallBackBox
    ) -> Result<(), std::string::String> {
        // Check the status.
        if self.status != RpcCoreStatus::Stopped {
            return Err("The RPC core is not stopped.".to_string());
        }

        // Check the callback name.
        if cb_index == 0 {
            return Err("The callback index is empty.".to_string());
        }

        // Register the callback function.
        {
            let _unrefed_lock = 
                self.runtime_lock.lock().unwrap();
            unsafe { 
                let cb_map_mut = 
                    get_mut_from_immut(&self.cb_map);
                cb_map_mut.insert(cb_index, cb); 
            }
        }

        Ok(())
    }

    pub fn get_req_index(&self) -> u64 {
        self.req_counter.fetch_add(
            1, 
            std::sync::atomic::Ordering::SeqCst
        )
    }

    pub fn get_cb_by_reqtype(
        &self, 
        req_type: u8
    ) -> Option<&CallBackBox> {
        trace!("get_cb_by_reqtype: {:?}", req_type);

        self.cb_map.get(&req_type)
    }

}

pub unsafe fn get_mut_from_immut<T>(immut: &T) -> &mut T {
    let immut_ptr = immut as *const T;
    let mut_ptr = immut_ptr as *mut T;
    &mut *mut_ptr
}
