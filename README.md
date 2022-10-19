# Secure RPC 
Secure RPC service. 

src/core/ 
srpc_core.rs 
RPC service kernel routine. 

srpc_dispatcher.rs 
Features RPC requests/ responses dispatching.   

srpc_session.rs 
Context of RPC connection to the remote ends. 

src/msg/ 
srpc_msg.rs 
RPC message formats and ser/de. 

src/tee/ 
srpc_tee_sgx.rs
Enclave functionalities, to be developed. 


