#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LikeKind {
    Wall,
    Photo,
    Video,
    Comment,
}

impl LikeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Photo => "photo",
            Self::Video => "video",
            Self::Comment => "comment",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "wall" | "post" => Some(Self::Wall),
            "photo" => Some(Self::Photo),
            "video" => Some(Self::Video),
            "comment" => Some(Self::Comment),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LikeTarget {
    pub kind: LikeKind,
    pub owner_id: i64,
    pub object_id: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct LikeState {
    pub liked: bool,
    pub count: i32,
}

#[derive(Clone, Debug)]
pub struct LikeFlags {
    pub count: i32,
    pub liked: bool,
}

impl Default for LikeFlags {
    fn default() -> Self {
        Self {
            count: 0,
            liked: false,
        }
    }
}
