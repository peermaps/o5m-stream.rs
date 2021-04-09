use crate::{Dataset,Info,Element};
type Strings = std::collections::VecDeque<(String,String)>;

type Error = Box<dyn std::error::Error+Send+Sync>;

#[derive(Clone,Debug)]
pub struct ParseError {
  pub message: String,
}
impl ParseError {
  fn new(msg: &str) -> Box<Self> {
    Box::new(Self { message: msg.into() })
  }
}
impl std::error::Error for ParseError {}
impl std::fmt::Display for ParseError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write![f, "o5m DecoderError: {}", &self.message]
  }
}

pub fn info(buf: &[u8], prev: &Option<Dataset>, strings: &mut Strings)
-> Result<(usize,(u64,Option<Info>)),Error> {
  let mut offset = 0;
  let mut info = Info::new();
  let id = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    (x + (match prev {
      Some(Dataset::Node(node)) => node.id,
      Some(Dataset::Way(way)) => way.id,
      Some(Dataset::Relation(relation)) => relation.id,
      _ => 0,
    } as i64)) as u64
  };
  info.version = {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 { return Ok((offset, (id, None))) }
    Some(x)
  };
  let prev_info = match prev {
    Some(Dataset::Node(node)) => node.get_info(),
    Some(Dataset::Way(way)) => way.get_info(),
    Some(Dataset::Relation(relation)) => relation.get_info(),
    _ => None
  };
  info.timestamp = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    let p = prev_info.and_then(|info| {
      Some(info.timestamp.unwrap_or(0))
    }).unwrap_or(0);
    if x + p == 0 { return Ok((offset, (id, Some(info)))) }
    Some(x + p)
  };
  info.changeset = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    let p = prev_info.and_then(|info| {
      Some(info.changeset.unwrap_or(0))
    }).unwrap_or(0) as i64;
    Some((x + p) as u64)
  };
  {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 {
      let (s,x) = unsigned(&buf[offset..])?;
      offset += s;
      info.uid = Some(x);
      let i = buf[offset..].iter().position(|p| *p == 0x00).unwrap_or(buf.len());
      info.user = Some(std::str::from_utf8(&buf[offset..i])?.to_string());
      offset = i;
    } else {
      //self.fields.uid = self.unsigned(self.strings[x].0);
      //self.fields.user = self.strings[x].1;
      unimplemented![];
    }
  }
  Ok((offset, (id, Some(info))))
}

type Tags = std::collections::HashMap<String,String>;

pub fn tags(buf: &[u8], strings: &mut Strings) -> Result<(usize,Tags),Error> {
  let mut tags = std::collections::HashMap::new();
  let mut offset = 0;
  while offset < buf.len() {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 {
      let i = buf[offset..].iter().position(|p| *p == 0x00).unwrap_or(buf.len());
      let key = std::str::from_utf8(&buf[offset..i])?.to_string();
      offset = i;
      let j = buf[offset..].iter().position(|p| *p == 0x00).unwrap_or(buf.len());
      let value = std::str::from_utf8(&buf[offset..j])?.to_string();
      offset = j;
      tags.insert(key, value);
    } else {
      // let (key,value) = self.strings[x];
      // self.fields.tags.insert(key, value);
      unimplemented![];
    }
  }
  Ok((offset,tags))
}

pub fn signed(buf: &[u8]) -> Result<(usize,i64),Error> {
  let mut value = 0i64;
  let mut npow = 1i64;
  let mut sign = 1i64;
  for (i,b) in buf.iter().enumerate() {
    value += ((*b as i64) & (match npow { 1 => 0x7e, _ => 0x7f })) * npow;
    if npow == 1 && (b & 0x01) == 1 {
      sign = -1;
    }
    npow *= 0x80;
    if *b < 0x80 {
      value = value * sign / 2;
      return Ok((i+1,value));
    }
  }
  Err(ParseError::new("unterminated signed integer"))
}

pub fn unsigned(buf: &[u8]) -> Result<(usize,u64),Error> {
  let mut value = 0u64;
  let mut npow = 1u64;
  for (i,b) in buf.iter().enumerate() {
    value += (*b as u64) & (match npow { 1 => 0x7e, _ => 0x7f }) * npow;
    npow *= 0x80;
    if *b < 0x80 {
      return Ok((i+1,value));
    }
  }
  Err(ParseError::new("unterminated unsigned integer"))
}
