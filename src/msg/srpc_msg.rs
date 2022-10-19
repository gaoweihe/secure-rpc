use serde::{Serialize, Deserialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcMsgPayload
{
    pub msg_data: Vec<u8>, 
}

impl RpcMsgPayload
{
    pub fn default() -> RpcMsgPayload
    {
        RpcMsgPayload
        {
            msg_data: Vec::new(), 
        }
    }

    pub fn set_data(&mut self, data: Vec<u8>)
    {
        self.msg_data = data;
    }
}

unsafe impl Send for RpcMsgPayload {}

#[derive(Debug)]
pub struct RpcMsgHandle
{
    pub msg_type: RpcMsgType, 
    pub peer_id: u32,
    pub peer_uri: std::string::String,
    pub msg: RpcOnceMsg,
}

impl RpcMsgHandle
{
    pub fn default() -> RpcMsgHandle
    {
        RpcMsgHandle
        {
            msg_type: RpcMsgType::Request,
            peer_id: 0,
            peer_uri: "".to_string(),
            msg: RpcOnceMsg::default(),
        }
    }

    pub fn set_msg(&mut self, msg: RpcOnceMsg)
    {
        self.msg = msg;
    }
}

unsafe impl Send for RpcMsgHandle {}


#[derive(Debug, PartialEq)]
pub enum RpcMsgType
{
    // Request message type.
    Request = 0,
    // Response message type.
    Response = 1,
    // Notification message type.
    Notification = 2, 
}

pub enum RpcMsg {
    Once(RpcOnceMsg),
    Stream(RpcStreamMsg),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcOnceMsg
{
    pub req_type: u8,
    pub src_id: u32, // optional 
    pub msg_id: u64, // bound to the client node 
    pub payload: RpcMsgPayload,
}

impl RpcOnceMsg
{
    pub fn default() -> RpcOnceMsg
    {
        RpcOnceMsg
        {
            req_type: 0,
            src_id: 0,
            msg_id: 0,
            payload: RpcMsgPayload::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcStreamMsg
{
    pub req_type: u8,
    pub req_id: u64, 
    pub seq_id: u64, 
    pub src_id: u32, // optional 
    pub msg_id: u64, // bound to the client node 
    pub payload: RpcMsgPayload,
}

impl RpcStreamMsg
{
    pub fn default() -> RpcStreamMsg
    {
        RpcStreamMsg
        {
            req_type: 0,
            req_id: 0,
            seq_id: 0,
            src_id: 0,
            msg_id: 0,
            payload: RpcMsgPayload::default(),
        }
    }
}

