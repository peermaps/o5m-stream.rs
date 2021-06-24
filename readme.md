# o5m-stream

streaming async o5m decoder

# example

``` rust
use async_std::{prelude::*,fs::File,io};

type Error = Box<dyn std::error::Error+Send+Sync>;
type R = Box<dyn io::Read+Send+Unpin>;

#[async_std::main]
async fn main() -> Result<(),Error> {
  let args = std::env::args().collect::<Vec<String>>();
  let infile: R = match args.get(1).unwrap_or(&"-".into()).as_str() {
    "-" => Box::new(io::stdin()),
    x => Box::new(File::open(x).await?),
  };
  let mut stream = o5m_stream::decode(infile);
  while let Some(result) = stream.next().await {
    let r = result?;
    println!["{:?}", r];
  }
  Ok(())
}
```

# license

bsd
