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
      state: State::Begin(),
      queue: VecDeque::new(),
      data_type: None,
      len: 0,
      npow: 1,
      chunk: vec![],
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
          self.chunk.extend_from_slice(&self.buffer[i..j]);
          self.size += j-i;
          if self.size >= self.len {
            if let Some(data) = self.flush()? {
              self.prev = Some(data.clone());
              self.queue.push_back(data);
            }
            self.state = State::Type();
            self.len = 0;
            self.size = 0;
            self.chunk.clear();
          }
          i = j - 1;
        } else if self.state == State::End() && b != 0xfe {
          return err(&format!["last byte in frame. expected: 0xf3, got: 0x{:02x}", b]);
        } else if self.state == State::End() {
          // ...
        }
        i += 1;
      }
    }
    Ok(None)
  }
  fn flush(&mut self) -> Result<Option<Dataset>,Error> {
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
        }
      },
      Some(DatasetType::Way()) => {
        let (s,(id,info)) = parse::info(&buf[offset..], &self.prev, &mut self.strings)?;
        offset += s;
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
        // reflen is the number of BYTES, not the number of refs
        let (s,reflen) = parse::unsigned(&buf[offset..])?;
        offset += s;
        let mut members = vec![];
        let mut prev_id = match &self.prev {
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
              self.strings.push_front((mbytes.to_vec(),vec![]));
              if self.strings.len() > 15_000 { self.strings.pop_back(); }
              mbytes
            } else {
							let pair = self.strings.get((x as usize)-1);
							if pair.is_none() {
								return err(&format!["string at index {} not available", x]);
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
              x => {
                return err(&format!["expected 0x30, 0x31, or 0x32 ('0','1', or '2') for \
                  element type. got: 0x{:02x}", mstring[0]]);
              }
            },
            role: String::from_utf8(mstring[1..].to_vec())?,
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
