use std::io;

fn main() -> io::Result<()>
{
    tonic_build::compile_protos("bandwidth/proto/precomm.proto")?;
    Ok(())
}
