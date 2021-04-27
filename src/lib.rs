//! # o5m-stream
//!
//! streaming async o5m decoder
//!
//! # example
//!
//! ``` rust,no_run
//! use async_std::{prelude::*,fs::File,io};
//!
//! type Error = Box<dyn std::error::Error+Send+Sync>;
//! type R = Box<dyn io::Read+Unpin>;
//!
//! #[async_std::main]
//! async fn main() -> Result<(),Error> {
//!   let args = std::env::args().collect::<Vec<String>>();
//!   let infile: R = match args.get(1).unwrap_or(&"-".into()).as_str() {
//!     "-" => Box::new(io::stdin()),
//!     x => Box::new(File::open(x).await?),
//!   };
//!   let mut stream = o5m_stream::decode(infile);
//!   while let Some(result) = stream.next().await {
//!     let r = result?;
//!     println!["{:?}", r];
//!   }
//!   Ok(())
//! }
//! ```

#![feature(async_closure,backtrace)]
use async_std::{prelude::*,stream::Stream,io};
use std::collections::VecDeque;

mod unfold;
mod data;
pub use data::*;
pub mod parse;

type Error = Box<dyn std::error::Error+Send+Sync>;

pub type DecodeItem = Result<Dataset,DecodeError>;
pub type DecodeStream = Box<dyn Stream<Item=DecodeItem>+Unpin>;

#[derive(Clone,PartialEq,Debug)]
enum State { Begin(), Type(), Len(), Data(), End() }

use std::backtrace::Backtrace;

#[derive(thiserror::Error)]
pub enum DecodeError {
  #[error("string at index {index} not available")]
  StringUnavailable {
    index: usize,
    #[backtrace] backtrace: Backtrace,
  },
  #[error("{info:?}. expected: 0x{expected:02x}, received: 0x{received:02x}")]
  UnexpectedByte {
    info: String,
    expected: u8,
    received: u8,
    #[backtrace] backtrace: Backtrace,
  },
  #[error("expected 0x30, 0x31, or 0x32 for element type. \
    received: 0x{received:02x}\n{backtrace}")]
  UnexpectedElementType {
    received: u8,
    #[backtrace] backtrace: Backtrace,
  },
  #[error("stream read error {source:?}")]
  StreamReadError { #[source] source: Box<Error> },
  #[error("string encoding error {source:?}")]
  StringEncodingError { #[source] source: Box<Error> },
  #[error("unterminated signed integer\n{backtrace}")]
  UnterminatedSignedInteger { #[backtrace] backtrace: Backtrace },
  #[error("unterminated unsigned integer\n{backtrace}")]
  UnterminatedUnsignedInteger { #[backtrace] backtrace: Backtrace },
}

impl std::fmt::Debug for DecodeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    std::fmt::Display::fmt(self, f)
  }
}

struct Decoder {
  reader: Box<dyn io::Read+Unpin>,
  buffer: Vec<u8>,
  index: usize,
  buffer_len: usize,
  state: State,
  data_type: Option<DatasetType>,
  len: usize,
  npow: u64,
  chunk: Vec<u8>,
  size: usize,
  strings: VecDeque<(Vec<u8>,Vec<u8>)>,
  prev: Option<Dataset>,
}

impl Decoder {
  pub fn new(reader: Box<dyn io::Read+Unpin>) -> Self {
    Self {
      reader,
      buffer: vec![0;4096],
      index: 0,
      buffer_len: 0,
      state: State::Begin(),
      data_type: None,
      len: 0,
      npow: 1,
      chunk: vec![],
      size: 0,
      strings: VecDeque::new(),
      prev: None,
    }
  }
  pub async fn next_item(&mut self) -> Result<Option<Dataset>,DecodeError> {
    loop {
      if self.index >= self.buffer_len {
        self.buffer_len = self.reader.read(&mut self.buffer).await
          .map_err(|e| DecodeError::StreamReadError { source: Box::new(e.into()) })?;
        self.index = 0;
        if self.buffer_len == 0 { break }
      }
      while self.index < self.buffer_len {
        let b = self.buffer[self.index];
        if self.state == State::Begin() && b != 0xff {
          return Err(DecodeError::UnexpectedByte {
            info: "first byte in frame".to_string(),
            expected: 0xff,
            received: b,
            backtrace: Backtrace::capture(),
          });
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
          let j = self.buffer_len.min(self.index+self.len-self.size);
          self.chunk.extend_from_slice(&self.buffer[self.index..j]);
          self.size += j-self.index;
          if self.size >= self.len {
            let res = self.flush()?;
            self.state = State::Type();
            self.len = 0;
            self.size = 0;
            self.chunk.clear();
            if let Some(data) = res {
              self.prev = Some(data.clone());
              self.index = j;
              return Ok(Some(data));
            }
          }
          self.index = j - 1;
        } else if self.state == State::End() && b != 0xfe {
          return Err(DecodeError::UnexpectedByte {
            info: "last byte in frame".to_string(),
            expected: 0xf3,
            received: b,
            backtrace: Backtrace::capture(),
          });
        } else if self.state == State::End() {
          // ...
        }
        self.index += 1;
      }
    }
    Ok(None)
  }
  fn flush(&mut self) -> Result<Option<Dataset>,DecodeError> {
    let mut offset = 0;
    let buf = &self.chunk;
    Ok(match self.data_type {
      Some(DatasetType::Node()) => {
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
        if offset == buf.len() {
          Some(Dataset::Node(Node {
            id,
            info,
            data: None,
            tags: std::collections::HashMap::new(),
          }))
        } else {
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
                .and_then(|data| Some(data.latitude)),
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
        }
      },
      Some(DatasetType::Way()) => {
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
        if offset == buf.len() {
          return Ok(Some(Dataset::Way(Way {
            id,
            info,
            data: None,
            tags: std::collections::HashMap::new(),
          })));
        }
        // reflen is the number of BYTES, not the number of refs
        let (s,reflen) = parse::unsigned(&buf[offset..])?;
        offset += s;
        let mut refs = vec![];
        let mut prev_ref = match &self.prev {
          Some(Dataset::Way(way)) => way.data.as_ref().and_then(|d| {
            d.refs.last().and_then(|r| Some(*r))
          }).unwrap_or(0),
          _ => 0
        };
        let ref_end = offset + reflen as usize;
        while offset < ref_end {
          let (s,x) = parse::signed(&buf[offset..])?;
          offset += s;
          let r = (x + (prev_ref as i64)) as u64;
          refs.push(r);
          prev_ref = r;
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
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
        if offset == buf.len() {
          return Ok(Some(Dataset::Relation(Relation {
            id,
            info,
            data: None,
            tags: std::collections::HashMap::new(),
          })));
        }
        // reflen is the number of BYTES, not the number of refs
        let (s,reflen) = parse::unsigned(&buf[offset..])?;
        offset += s;
        let mut members = vec![];
        let prev_id = match &self.prev {
          Some(Dataset::Relation(rel)) => rel.data.as_ref().and_then(|d| {
            d.members.last().and_then(|m| Some(m.id))
          }).unwrap_or(0),
          _ => 0
        };
        let ref_end = offset + reflen as usize;
        while offset < ref_end {
          let m_id = {
            let (s,x) = parse::signed(&buf[offset..])?;
            offset += s;
            (x + (prev_id as i64)) as u64
          };
          let mstring = {
            let (s,x) = parse::unsigned(&buf[offset..])?;
            offset += s;
            if x == 0 {
              let i = offset + buf[offset..].iter()
                .position(|p| *p == 0x00).unwrap_or(buf.len()-offset);
              let mbytes = &buf[offset..i];
              offset = i+1;
              if mbytes.len() <= 250 {
                self.strings.push_front((mbytes.to_vec(),vec![]));
                if self.strings.len() > 15_000 { self.strings.pop_back(); }
              }
              mbytes
            } else {
              let pair = self.strings.get((x as usize)-1);
              if pair.is_none() {
                return Err(DecodeError::StringUnavailable {
                  index: x as usize,
                  backtrace: Backtrace::capture(),
                });
              }
              &pair.unwrap().0
            }
          };
          members.push(RelationMember {
            id: m_id,
            element_type: match mstring[0] {
              0x30 => ElementType::Node(),
              0x31 => ElementType::Way(),
              0x32 => ElementType::Relation(),
              x => return Err(DecodeError::UnexpectedElementType {
                received: x,
                backtrace: Backtrace::capture(),
              }),
            },
            role: String::from_utf8(mstring[1..].to_vec())
              .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?,
          });
        }
        let (_,tags) = parse::tags(&buf[offset..], &mut self.strings)?;
        Some(Dataset::Relation(Relation {
          id,
          info,
          data: Some(RelationData { members }),
          tags
        }))
      },
      Some(DatasetType::Timestamp()) => {
        let (_,time) = parse::signed(&buf[offset..])?;
        Some(Dataset::Timestamp(Timestamp { time }))
      },
      Some(DatasetType::BBox()) => {
        let (s,x1) = parse::signed(&buf[offset..])?;
        offset += s;
        let (s,y1) = parse::signed(&buf[offset..])?;
        offset += s;
        let (s,x2) = parse::signed(&buf[offset..])?;
        offset += s;
        let (_,y2) = parse::signed(&buf[offset..])?;
        Some(Dataset::BBox(BBox {
          x1: x1 as i32,
          y1: y1 as i32,
          x2: x2 as i32,
          y2: y2 as i32,
        }))
      },
      Some(DatasetType::Header()) => None,
      Some(DatasetType::Sync()) => None,
      Some(DatasetType::Jump()) => None,
      Some(DatasetType::Reset()) => None,
      None => None,
    })
  }
}

/// Transform the given binary stream `reader` into an stream of fallible `Dataset` items.
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
