#[derive(Clone,PartialEq,Debug)]
pub enum Dataset {
  Node(Node),
  Way(Way),
  Relation(Relation),
  BBox(BBox),
  Timestamp(Timestamp),
}

#[derive(Clone,PartialEq,Debug)]
pub enum DatasetType {
  Node(), Way(), Relation(), BBox(), Timestamp(),
  Header(), Sync(), Jump(), Reset(),
}

#[derive(Clone,PartialEq,Debug)]
pub enum ElementType {
  Node(), Way(), Relation(),
}

pub type Tags = std::collections::HashMap<String,String>;

pub trait Element {
  fn get_id(&self) -> u64;
  fn get_info<'a>(&'a self) -> Option<&'a Info>;
  fn get_type(&self) -> ElementType;
  fn get_tags<'a>(&'a self) -> &'a Tags;
}

#[derive(Clone,PartialEq,Debug)]
pub struct Info {
  pub version: Option<u64>,
  pub timestamp: Option<i64>,
  pub changeset: Option<u64>,
  pub uid: Option<u64>,
  pub user: Option<String>,
}
impl Info {
  pub fn new() -> Self {
    Self {
      version: None,
      timestamp: None,
      changeset: None,
      uid: None,
      user: None
    }
  }
}

#[derive(Clone,PartialEq,Debug)]
pub struct Node {
  pub id: u64,
  pub info: Option<Info>,
  pub data: Option<NodeData>,
  pub tags: Tags,
}
#[derive(Clone,PartialEq,Debug)]
pub struct NodeData {
  pub longitude: i32,
  pub latitude: i32,
}
impl Element for Node {
  fn get_id(&self) -> u64 { self.id }
  fn get_info<'a>(&'a self) -> Option<&'a Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Node() }
  fn get_tags<'a>(&'a self) -> &'a Tags { &self.tags }
}

#[derive(Clone,PartialEq,Debug)]
pub struct Way {
  pub id: u64,
  pub info: Option<Info>,
  pub data: Option<WayData>,
  pub tags: Tags,
}
#[derive(Clone,PartialEq,Debug)]
pub struct WayData {
  pub refs: Vec<u64>
}
impl Element for Way {
  fn get_id(&self) -> u64 { self.id }
  fn get_info<'a>(&'a self) -> Option<&'a Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Way() }
  fn get_tags<'a>(&'a self) -> &'a Tags { &self.tags }
}

#[derive(Clone,PartialEq,Debug)]
pub struct Relation {
  pub id: u64,
  pub info: Option<Info>,
  pub data: Option<RelationData>,
  pub tags: Tags,
}
#[derive(Clone,PartialEq,Debug)]
pub struct RelationData {
  pub members: Vec<RelationMember>
}
#[derive(Clone,PartialEq,Debug)]
pub struct RelationMember {
  pub id: u64,
  pub element_type: ElementType,
  pub role: String,
}
impl Element for Relation {
  fn get_id(&self) -> u64 { self.id }
  fn get_info<'a>(&'a self) -> Option<&'a Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Relation() }
  fn get_tags<'a>(&'a self) -> &'a Tags { &self.tags }
}

#[derive(Clone,PartialEq,Debug)]
pub struct BBox {
  pub x1: i64,
  pub y1: i64,
  pub x2: i64,
  pub y2: i64,
}

#[derive(Clone,PartialEq,Debug)]
pub struct Timestamp {
  pub time: i64,
}
