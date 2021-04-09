#![feature(async_closure)]
use async_std::{prelude::*,stream::Stream,io};
use std::collections::VecDeque;

mod unfold;
mod data;
pub use data::*;
pub mod parse;

type Error = Box<dyn std::error::Error+Send+Sync>;

pub type DecodeItem = Result<Dataset,Error>;
pub type DecodeStream = Box<dyn Stream<Item=DecodeItem>+Unpin>;

#[derive(Clone,PartialEq,Debug)]
enum State { Begin(), Type(), Len(), Data(), End() }

struct Decoder {
  reader: Box<dyn io::Read+Unpin>,
  buffer: Vec<u8>,
  queue: VecDeque<Dataset>,
  state: State,
  data_type: Option<DatasetType>,
  len: usize,
  npow: u64,
  chunks: Vec<Vec<u8>>,
  size: usize,
  strings: VecDeque<(String,String)>,
  prev: Option<Dataset>,
}

impl Decoder {
  pub fn new(reader: Box<dyn io::Read+Unpin>) -> Self {
    Self {
      reader,
      buffer: vec![0;4096],
      state: State::Begin(),
      queue: VecDeque::new(),
      data_type: None,
      len: 0,
      npow: 1,
      chunks: vec![],
      size: 0,
      strings: VecDeque::new(),
      prev: None,
    }
  }
  pub async fn next_item(&mut self) -> Result<Option<Dataset>,Error> {
    loop {
      if let Some(item) = self.queue.pop_front() {
        return Ok(Some(item));
      }
      let n = self.reader.read(&mut self.buffer).await?;
      if n == 0 { break }
      let mut i = 0;
      while i < n {
        let b = self.buffer[i];
        if self.state == State::Begin() && b != 0xff {
          return err(&format!["first byte in frame. expected: 0xff, got: 0x{:02x}", b]);
        } else if self.state == State::Begin() {
          self.state = State::Type();
        } else if self.state == State::Type() && b == 0xff { // reset
          self.state = State::Type();
          self.prev = None;
        } else if self.state == State::Type() {
          self.state = State::Len();
          self.data_type = match b {
            0x10 => Some(DatasetType::Node()),
            0x11 => Some(DatasetType::Way()),
            0x12 => Some(DatasetType::Relation()),
            0xdb => Some(DatasetType::BBox()),
            0xdc => Some(DatasetType::Timestamp()),
            0xe0 => Some(DatasetType::Header()),
            0xee => Some(DatasetType::Sync()),
            0xef => Some(DatasetType::Jump()),
            0xff => Some(DatasetType::Reset()),
            _ => None,
          };
        } else if self.state == State::Len() {
          self.len += ((b & 0x7f) as usize) * (self.npow as usize);
          self.npow *= 0x80;
          if b < 0x80 {
            self.npow = 1;
            self.state = State::Data();
          }
        } else if self.state == State::Data() {
          let j = n.min(i+self.len-self.size);
          self.chunks.push(self.buffer[i..j].to_vec());
          self.size += j-i;
          if self.size >= self.len {
            let buf = self.chunks.concat();
            self.chunks.clear();
            if let Some(data) = self.flush(&buf)? {
              self.queue.push_back(data);
            }
            self.state = State::Type();
            self.len = 0;
            self.size = 0;
          }
          i = j - 1;
        } else if self.state == State::End() && b != 0xfe {
          return err(&format!["last byte in frame. expected: 0xf3, got: 0x{:02x}", b]);
        } else if self.state == State::End() {
          // ...
        }
        i += 1;
      }
      eprintln!["n={}",n];
    }
    Ok(None)
  }
  fn flush(&mut self, buf: &[u8]) -> Result<Option<Dataset>,Error> {
    let mut offset = 0;
    Ok(match self.data_type {
      Some(DatasetType::Timestamp()) => {
        let (_,time) = parse::signed(&buf[offset..])?;
        Some(Dataset::Timestamp(Timestamp { time }))
      },
      Some(DatasetType::Node()) => {
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
        let longitude = {
          let (s,x) = parse::signed(&buf[offset..])?;
          offset += s;
          (x + (match &self.prev {
            Some(Dataset::Node(node)) => node.data.as_ref()
              .and_then(|data| Some(data.longitude)),
            _ => None,
          }.unwrap_or(0) as i64)) as i32
        };
        let latitude = {
          let (s,x) = parse::signed(&buf[offset..])?;
          offset += s;
          (x + (match &self.prev {
            Some(Dataset::Node(node)) => node.data.as_ref()
              .and_then(|data| Some(data.longitude)),
            _ => None,
          }.unwrap_or(0) as i64)) as i32
        };
        let (_,tags) = parse::tags(&buf[offset..], &mut self.strings)?;
        Some(Dataset::Node(Node {
          id,
          info,
          data: Some(NodeData { longitude, latitude }),
          tags,
        }))
      },
      Some(DatasetType::Way()) => {
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
        let (s,reflen) = parse::unsigned(&buf[offset..])?;
        offset += s;
        let mut refs = vec![];
        for i in 0..reflen {
          let (s,x) = parse::signed(&buf[offset..])?;
          offset += s;
          let r = (x + (match (i,&self.prev) {
            (0,Some(Dataset::Way(way))) => way.data.as_ref().and_then(|d| {
              d.refs.last().and_then(|r| Some(*r))
            }),
            (0,_) => None,
            _ => refs.last().and_then(|r| Some(*r)),
          }.unwrap_or(0) as i64)) as u64;
          refs.push(r);
        }
        let (_,tags) = parse::tags(&buf[offset..], &mut self.strings)?;
        Some(Dataset::Way(Way {
          id,
          info,
          data: Some(WayData { refs }),
          tags
        }))
      },
      Some(DatasetType::Relation()) => {
        /*
        let (_,tags) = parse::tags(&buf[offset..], &mut self.strings)?;
        Ok(Some(Dataset::Relation(Relation {
          id,
          info,
          data: Some(RelationData { members }),
          tags
        })))
        */
        None
      },
      Some(DatasetType::BBox()) => {
        None
      },
      _ => None,
    })
  }
}

#[derive(Debug)]
pub struct DecoderError { message: String }
impl std::error::Error for DecoderError {}
impl std::fmt::Display for DecoderError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write![f, "o5m DecoderError: {}", &self.message]
  }
}
fn err<T>(message: &str) -> Result<T,Box<dyn std::error::Error+Send+Sync>> {
  Err(Box::new(DecoderError { message: message.to_string() }))
}

pub fn decode(reader: Box<dyn io::Read+Unpin>) -> DecodeStream {
  let state = Decoder::new(reader);
  Box::new(unfold::unfold(state, async move |mut qs| {
    match qs.next_item().await {
      Ok(None) => None,
      Ok(Some(x)) => Some((Ok(x),qs)),
      Err(e) => Some((Err(e),qs)),
    }
  }))
}
