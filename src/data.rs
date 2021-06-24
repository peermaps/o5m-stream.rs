#[derive(Clone,PartialEq,Debug)]
pub enum Dataset {
  Node(Node),
  Way(Way),
  Relation(Relation),
  BBox(BBox),
  Timestamp(Timestamp),
}
impl Dataset {
  pub fn get_id(&self) -> Option<u64> {
    match self {
      Self::Node(node) => Some(node.id),
      Self::Way(way) => Some(way.id),
      Self::Relation(relation) => Some(relation.id),
      _ => None,
    }
  }
  pub fn get_info(&self) -> Option<Info> {
    match self {
      Self::Node(node) => node.info.clone(),
      Self::Way(way) => way.info.clone(),
      Self::Relation(relation) => relation.info.clone(),
      _ => None,
    }
  }
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
  fn get_info(&'_ self) -> Option<&'_ Info>;
  fn get_type(&self) -> ElementType;
  fn get_tags(&'_ self) -> &'_ Tags;
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

impl Default for Info {
  fn default() -> Self { Self::new() }
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
impl NodeData {
  pub fn get_longitude(&self) -> f32 {
    (self.longitude as f32) / 1.0e7
  }
  pub fn get_latitude(&self) -> f32 {
    (self.latitude as f32) / 1.0e7
  }
}
impl Element for Node {
  fn get_id(&self) -> u64 { self.id }
  fn get_info(&'_ self) -> Option<&'_ Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Node() }
  fn get_tags(&'_ self) -> &'_ Tags { &self.tags }
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
  fn get_info(&'_ self) -> Option<&'_ Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Way() }
  fn get_tags(&'_ self) -> &'_ Tags { &self.tags }
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
  fn get_info(&'_ self) -> Option<&'_ Info> {
    self.info.as_ref()
  }
  fn get_type(&self) -> ElementType { ElementType::Relation() }
  fn get_tags(&'_ self) -> &'_ Tags { &self.tags }
}

#[derive(Clone,PartialEq,Debug)]
pub struct BBox {
  pub x1: i32,
  pub y1: i32,
  pub x2: i32,
  pub y2: i32,
}
impl BBox {
  pub fn get_x1(&self) -> f32 { self.x1 as f32 / 1.0e7 }
  pub fn get_y1(&self) -> f32 { self.y1 as f32 / 1.0e7 }
  pub fn get_x2(&self) -> f32 { self.x2 as f32 / 1.0e7 }
  pub fn get_y2(&self) -> f32 { self.y2 as f32 / 1.0e7 }
  pub fn get_bounds(&self) -> (f32,f32,f32,f32) {
    (self.get_x1(),self.get_y1(),self.get_x2(),self.get_y2())
  }
}

#[derive(Clone,PartialEq,Debug)]
pub struct Timestamp {
  pub time: i64,
}
