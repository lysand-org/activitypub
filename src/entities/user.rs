//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.10

use chrono::Utc;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub username: String,
    pub name: String,
    pub summary: Option<String>,
    pub url: String,
    pub public_key: String,
    pub private_key: Option<String>,
    #[sea_orm(column_type = "Timestamp")]
    pub last_refreshed_at: chrono::DateTime<Utc>,
    pub local: bool,
    pub follower_count: i32,
    pub following_count: i32,
    #[sea_orm(column_type = "Timestamp")]
    pub created_at: chrono::DateTime<Utc>,
    #[sea_orm(column_type = "Timestamp")]
    pub updated_at: Option<chrono::DateTime<Utc>>,
    pub following: Option<String>,
    pub followers: Option<String>,
    pub inbox: String,
    pub ap_json: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::post::Entity")]
    Post,
}

impl Related<super::post::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Post.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
