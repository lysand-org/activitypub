pub use sea_orm_migration::prelude::*;

mod m20220101_000001_post_table;
mod m20240417_230111_user_table;
mod m20240417_233430_post_user_keys;
mod m20240505_002524_user_follow_relation;
mod m20240626_030922_store_ap_json_in_posts;
mod m20240719_235452_user_ap_column;
mod m20240725_120932_follow_table_two_point_zero;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_post_table::Migration),
            Box::new(m20240417_230111_user_table::Migration),
            Box::new(m20240417_233430_post_user_keys::Migration),
            Box::new(m20240505_002524_user_follow_relation::Migration),
            Box::new(m20240626_030922_store_ap_json_in_posts::Migration),
            Box::new(m20240719_235452_user_ap_column::Migration),
            Box::new(m20240725_120932_follow_table_two_point_zero::Migration),
        ]
    }
}
