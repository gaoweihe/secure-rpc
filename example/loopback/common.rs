pub fn serialize_endpoint(
    endpoint: ibverbs::QueuePairEndpoint
) -> Vec<u8> {
    let mut serializer = flexbuffers::FlexbufferSerializer::new();
    endpoint.serialize(&mut serializer).unwrap();
    let endpoint_bin = serializer.view();
    let endpoint_bin_vec = endpoint_bin.to_vec();
    endpoint_bin_vec
}

pub fn deserialize_endpoint(
    endpoint_bin: Vec<u8>
) -> ibverbs::QueuePairEndpoint {
    let endpoint_bin_slice = endpoint_bin.as_slice();
    let reader = flexbuffers::Reader::get_root(
        endpoint_bin_slice
    ).unwrap();
    let endpoint = ibverbs::QueuePairEndpoint::deserialize(
        reader
    ).unwrap();
    endpoint
}