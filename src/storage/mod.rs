//! SQLite 存储层。
//!
//! 该层负责把领域模型映射到数据库记录，Web/API 和调度器不直接写 SQL。

pub mod alerts;
pub mod checks;
pub mod db;
pub mod migrations;
pub mod monitors;
