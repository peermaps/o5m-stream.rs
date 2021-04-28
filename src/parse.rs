use crate::{DecodeError,Info};
type Strings = std::collections::VecDeque<(Vec<u8>,Vec<u8>)>;
use std::backtrace::Backtrace;

pub fn info(buf: &[u8], prev_id: &Option<u64>, prev_info: &Option<Info>, strings: &mut Strings)
-> Result<(usize,(u64,Option<Info>)),DecodeError> {
  let mut offset = 0;
  let mut info = Info::new();
  let id = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    (x + prev_id.unwrap_or(0) as i64) as u64
  };
  info.version = {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 { return Ok((offset, (id, None))) }
    Some(x)
  };
  info.timestamp = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    let p = prev_info.as_ref().and_then(|info| {
      Some(info.timestamp.unwrap_or(0))
    }).unwrap_or(0);
    if x + p == 0 { return Ok((offset, (id, Some(info)))) }
    Some(x + p)
  };
  info.changeset = {
    let (s,x) = signed(&buf[offset..])?;
    offset += s;
    let p = prev_info.as_ref().and_then(|info| {
      Some(info.changeset.unwrap_or(0))
    }).unwrap_or(0) as i64;
    Some((x + p) as u64)
  };
  {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 {
      let (s,x) = unsigned(&buf[offset..])?;
      let uid_bytes = &buf[offset..offset+s];
      offset += s;
      info.uid = Some(x);
      if buf[offset] != 0 {
        return Err(DecodeError::UnexpectedByte {
          info: "decoding uid".to_string(),
          expected: 0,
          received: buf[offset],
          backtrace: Backtrace::capture(),
        });
      }
      offset += 1;
      let i = offset + buf[offset..].iter()
        .position(|p| *p == 0x00).unwrap_or(buf.len()-offset);
      info.user = Some(std::str::from_utf8(&buf[offset..i])
        .map_err(|e| DecodeError::StringEncodingError {
          source: Box::new(e.into())
        })?
        .to_string()
      );
      if uid_bytes.len() + (i-offset) <= 250 {
        strings.push_front((uid_bytes.to_vec(),buf[offset..i].to_vec()));
        if strings.len() > 15_000 { strings.pop_back(); }
      }
      offset = i+1;
    } else {
      let pair = strings.get((x as usize)-1);
      if pair.is_none() {
        return Err(DecodeError::StringUnavailable {
          index: x as usize,
          backtrace: Backtrace::capture(),
        });
      }
      let (uid_bytes,user_bytes) = pair.unwrap();
      info.uid = Some(unsigned(&uid_bytes)?.1);
      info.user = Some(String::from_utf8(user_bytes.to_vec())
        .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?
      );
    }
  }
  Ok((offset, (id, Some(info))))
}

type Tags = std::collections::HashMap<String,String>;

pub fn tags(buf: &[u8], strings: &mut Strings) -> Result<(usize,Tags),DecodeError> {
  let mut tags = std::collections::HashMap::new();
  let mut offset = 0;
  while offset < buf.len() {
    let (s,x) = unsigned(&buf[offset..])?;
    offset += s;
    if x == 0 {
      let i = offset + buf[offset..].iter()
        .position(|p| *p == 0x00).unwrap_or(buf.len()-offset);
      let key_bytes = &buf[offset..i];
      let key = std::str::from_utf8(key_bytes)
        .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?
        .to_string();
      offset = i+1;
      let j = offset + buf[offset..].iter()
        .position(|p| *p == 0x00).unwrap_or(buf.len()-offset);
      let value_bytes = &buf[offset..j];
      let value = std::str::from_utf8(value_bytes)
        .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?
        .to_string();
      offset = j+1;
      tags.insert(key, value);
      if key_bytes.len() + value_bytes.len() <= 250 {
        strings.push_front((key_bytes.to_vec(),value_bytes.to_vec()));
        if strings.len() > 15_000 { strings.pop_back(); }
      }
    } else {
      let pair = strings.get((x as usize)-1);
      if pair.is_none() {
        return Err(DecodeError::StringUnavailable {
          index: x as usize,
          backtrace: Backtrace::capture(),
        });
      }
      let (key_bytes,value_bytes) = pair.unwrap();
      let key = String::from_utf8(key_bytes.to_vec())
        .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?;
      let value = String::from_utf8(value_bytes.to_vec())
        .map_err(|e| DecodeError::StringEncodingError { source: Box::new(e.into()) })?;
      tags.insert(key, value);
    }
  }
  Ok((offset,tags))
}

pub fn signed(buf: &[u8]) -> Result<(usize,i64),DecodeError> {
  let mut value = 0;
  let mut lshift = 0;
  for (i,b) in buf.iter().enumerate() {
    value += ((*b as i64) & 0x7f) << lshift;
    lshift += 7;
    if *b < 0x80 {
      let sign = match value % 2 { 0 => 1, _ => -1 };
      value = value * sign / 2;
      if sign < 0 { value -= 1 }
      return Ok((i+1,value));
    }
  }
  Err(DecodeError::UnterminatedSignedInteger { backtrace: Backtrace::capture() })
}

pub fn unsigned(buf: &[u8]) -> Result<(usize,u64),DecodeError> {
  let mut value = 0;
  let mut lshift = 0;
  for (i,b) in buf.iter().enumerate() {
    value += ((*b as u64) & 0x7f) << lshift;
    lshift += 7;
    if *b < 0x80 {
      return Ok((i+1,value));
    }
  }
  Err(DecodeError::UnterminatedUnsignedInteger { backtrace: Backtrace::capture() })
}
