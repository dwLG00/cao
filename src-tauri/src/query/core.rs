use serde::{Serialize, Deserialize};
use std::{cmp::Ordering, default::Default};
use regex::Regex;
use sqlx::sqlite::SqlitePool;
use sqlx::{query_builder::QueryBuilder, Execute};

use chrono::{Utc, DateTime};

use anyhow::Result;

use super::super::tasks::core::TaskDescription;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum Availability {
    All,
    #[default]
    Incomplete,
    Available,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub enum OrderType {
    Due,
    Start,
    #[default]
    Captured,
    Scheduled,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OrderRequest {
    #[serde(default)]
    pub order: OrderType,
    pub ascending: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BrowseRequest {
    #[serde(default)]
    pub availability: Availability,
    #[serde(default)]
    pub order: OrderRequest,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub query_regexp: Option<String>,
    #[serde(default)]
    pub query_text: Option<String>,
}

impl BrowseRequest {
    fn compare_fn_dateoptions(main_a: Option<DateTime<Utc>>,
                              main_b: Option<DateTime<Utc>>,
                              backup_a: DateTime<Utc>,
                              backup_b: DateTime<Utc>) -> Ordering {
        if !main_a.is_none() && !main_b.is_none() {
            main_a.unwrap().timestamp_millis().cmp(&main_b.unwrap().timestamp_millis())
        } else if main_a.is_none() && !main_b.is_none() {
            Ordering::Greater
        } else if main_b.is_none() && !main_a.is_none() {
            Ordering::Less
        } else {
            backup_a.timestamp_millis().cmp(&backup_b.timestamp_millis())
       }
    }

    pub fn search_query(&self) -> String {
        let today = Utc::now();

        let mut query = {
            let mut query = QueryBuilder::new("SELECT * FROM tasks ");
            if self.availability != Availability::All || self.tags.len() != 0 {
                query.push(" WHERE ");
            }
            match self.availability {
                Availability::Incomplete => {
                    query.push(" complete == 0 ");
                },
                Availability::Available => {
                    query.push(" complete == 0 AND (start == NULL OR start < ?) ");
                    query.push_bind(today);
                },
                Availability::Done => {
                    query.push(" complete == 1 ");
                },
                Availability::All => {
                    query.push(" TRUE ");
                }
            }
            for tag in self.tags.iter() {
                query.push(" AND tag LIKE '%?%' ");
                query.push_bind(tag);
            }
            query
        };
        let order_direction = if self.order.ascending { "ASC" } else { "DESC" };
        match self.order.order {
            OrderType::Captured => {
                query.push(" ORDER BY captured ");
                query.push_bind(order_direction);
            },
            OrderType::Start => {
                query.push(" ORDER BY start ");
                query.push_bind(order_direction);
                query.push(" , captured ");
                query.push_bind(order_direction);
            },
            OrderType::Due => {
                query.push(" ORDER BY due ");
                query.push_bind(order_direction);
                query.push(" , captured ");
                query.push_bind(order_direction);
            },
            OrderType::Scheduled => {
                query.push(" ORDER BY schedule ");
                query.push_bind(order_direction);
                query.push(" , captured ");
                query.push_bind(order_direction);
            }
        }
        query.build().sql().into()
    }

    /// Query directly from the sqlite file
    /*
    pub fn execute_sqlite(&self, pool: &SqlitePool) -> Result<Vec<>>{
        // Filter by...
        // -> does it have the right availability?
        // -> does it contain the right tags?
        // -> does it match the regex? <== impl later
        // Sort by either
        // -> captured time
        // -> start time
        // -> due time
        // -> scheduled time

        let today = Utc::now();

        let availability_str = match self.availability {
            Availability::Incomplete => "completed = 0",
            Availability::Available => "completed = 0 AND (start = NULL OR start < )",
        };

        let mut filtered:Vec<_> = {
            // Filter by availability and tags
            let filtered_availability: Vec<_> = sqlx::query_as("SELECT * FROM tables WHERE ")
        }
    }
    */

    /// Use a QueryRequest to filter a list of tasks
    pub fn execute<'a>(&self, data: &'a[TaskDescription]) -> Result<Vec<&'a TaskDescription>>{
        let q = match &self.query_regexp {
            Some(x) => Some(Regex::new(&x)?),
            None => None
        };
        let today = Utc::now();
        
        let mut filtered:Vec<_> = data
            .iter()
            .filter(|x|
                    self.tags.iter()
                    .map(|y| x.tags.contains(y))
                    .fold(true, |acc, mk| acc && mk))
            .filter(|x| match &q { Some(y) => !y.captures(&x.content).is_none(), None => true })
            .filter(|x| match self.availability {
                Availability::Incomplete => !x.completed,
                Availability::Available => 
                    !x.completed &&
                    (x.start.is_none() ||
                     x.start.unwrap() < today),
                Availability::Done => x.completed,
                Availability::All => true,
            })
            .collect();

        filtered.sort_by(|x, y| {
            match self.order.order {
                OrderType::Captured => x.captured.timestamp_millis().cmp(&y.captured.timestamp_millis()),
                OrderType::Start => BrowseRequest::compare_fn_dateoptions(
                    x.start, y.start, x.captured, y.captured
                ),
                OrderType::Due => BrowseRequest::compare_fn_dateoptions(
                    x.due, y.due, x.captured, y.captured
                ),
                OrderType::Scheduled => BrowseRequest::compare_fn_dateoptions(
                    x.schedule, y.schedule, x.captured, y.captured
                )
            }
        });

        if !self.order.ascending { 
            filtered.reverse();
        }

        Ok(filtered)
    }
}

