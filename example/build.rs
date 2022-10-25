use std::io;

fn main() -> io::Result<()>
{
    tonic_build::compile_protos("loopback/proto/precomm.proto")?;
    Ok(())
}
