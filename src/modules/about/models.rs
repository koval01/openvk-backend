#[derive(Clone, Debug)]
pub struct PopularGroup {
    pub id: i64,
    pub name: String,
    pub members: i64,
}

#[derive(Clone, Debug)]
pub struct InstanceAbout {
    pub users: i64,
    pub online_users: i64,
    pub active_users: i64,
    pub groups: i64,
    pub wall_posts: i64,
    pub popular_groups: Vec<PopularGroup>,
}
