use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::TableState;

use filament_core::connection::FilamentConnection;
use filament_core::error::Result;
use filament_core::models::{
    AgentRun, Entity, EntityStatus, EntityType, RelationType, Reservation,
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tasks,
    Agents,
    Reservations,
}

impl Tab {
    pub const ALL: [Self; 3] = [Self::Tasks, Self::Agents, Self::Reservations];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Tasks => Self::Agents,
            Self::Agents => Self::Reservations,
            Self::Reservations => Self::Tasks,
        }
    }

    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Tasks => Self::Reservations,
            Self::Agents => Self::Tasks,
            Self::Reservations => Self::Agents,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tasks => "Tasks",
            Self::Agents => "Agents",
            Self::Reservations => "Reservations",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Tasks => 0,
            Self::Agents => 1,
            Self::Reservations => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub entity: Entity,
    pub blocked_by_count: usize,
    pub impact: usize,
}

#[derive(Debug, Clone)]
pub struct TaskFilter {
    pub status: Option<EntityStatus>,
}

impl TaskFilter {
    pub const fn cycle(&mut self) {
        self.status = match &self.status {
            Some(EntityStatus::Open) => Some(EntityStatus::InProgress),
            Some(EntityStatus::InProgress) => Some(EntityStatus::Blocked),
            Some(EntityStatus::Blocked) => Some(EntityStatus::Closed),
            Some(EntityStatus::Closed) => None,
            None => Some(EntityStatus::Open),
        };
    }

    #[must_use]
    pub const fn label(&self) -> &str {
        match &self.status {
            Some(EntityStatus::Open) => "open",
            Some(EntityStatus::InProgress) => "in_progress",
            Some(EntityStatus::Blocked) => "blocked",
            Some(EntityStatus::Closed) => "closed",
            None => "all",
        }
    }
}

impl Default for TaskFilter {
    fn default() -> Self {
        Self {
            status: Some(EntityStatus::Open),
        }
    }
}

pub struct App {
    pub conn: FilamentConnection,
    pub active_tab: Tab,
    pub should_quit: bool,
    pub tasks: Vec<TaskRow>,
    pub agent_runs: Vec<AgentRun>,
    pub reservations: Vec<Reservation>,
    pub task_table_state: TableState,
    pub agent_table_state: TableState,
    pub reservation_table_state: TableState,
    pub task_filter: TaskFilter,
    pub last_refresh: DateTime<Utc>,
    pub status_message: Option<String>,
    last_tick: Instant,
}

impl App {
    #[must_use]
    pub fn new(conn: FilamentConnection) -> Self {
        Self {
            conn,
            active_tab: Tab::Tasks,
            should_quit: false,
            tasks: Vec::new(),
            agent_runs: Vec::new(),
            reservations: Vec::new(),
            task_table_state: TableState::default(),
            agent_table_state: TableState::default(),
            reservation_table_state: TableState::default(),
            task_filter: TaskFilter::default(),
            last_refresh: Utc::now(),
            status_message: None,
            last_tick: Instant::now(),
        }
    }

    pub fn should_auto_refresh(&self) -> bool {
        self.last_tick.elapsed() >= REFRESH_INTERVAL
    }

    pub async fn refresh_all(&mut self) {
        self.refresh_tasks().await;
        self.refresh_agents().await;
        self.refresh_reservations().await;
        self.last_refresh = Utc::now();
        self.last_tick = Instant::now();
    }

    pub async fn refresh_tasks(&mut self) {
        let result = self
            .conn
            .list_entities(Some(EntityType::Task), self.task_filter.status.clone())
            .await;

        match result {
            Ok(entities) => {
                let task_count = entities.len();
                let use_impact = task_count <= 50;
                let mut rows = Vec::with_capacity(task_count);

                for entity in entities {
                    let entity_id = entity.id().as_str().to_string();

                    let blocked_by_count = self
                        .conn
                        .list_relations(&entity_id)
                        .await
                        .map(|rels| {
                            rels.iter()
                                .filter(|r| r.relation_type == RelationType::DependsOn)
                                .count()
                        })
                        .unwrap_or(0);

                    let impact = if use_impact {
                        self.conn.impact_score(&entity_id).await.unwrap_or(0)
                    } else {
                        0
                    };

                    rows.push(TaskRow {
                        entity,
                        blocked_by_count,
                        impact,
                    });
                }

                self.tasks = rows;
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub async fn refresh_agents(&mut self) {
        match self.conn.list_running_agents().await {
            Ok(runs) => {
                self.agent_runs = runs;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub async fn refresh_reservations(&mut self) {
        match self.conn.list_reservations(None).await {
            Ok(res) => {
                self.reservations = res;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub fn select_next(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let state = self.current_table_state_mut();
        let i = state
            .selected()
            .map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
        state.select(Some(i));
    }

    pub fn select_prev(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let state = self.current_table_state_mut();
        let i = state
            .selected()
            .map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
        state.select(Some(i));
    }

    const fn current_list_len(&self) -> usize {
        match self.active_tab {
            Tab::Tasks => self.tasks.len(),
            Tab::Agents => self.agent_runs.len(),
            Tab::Reservations => self.reservations.len(),
        }
    }

    const fn current_table_state_mut(&mut self) -> &mut TableState {
        match self.active_tab {
            Tab::Tasks => &mut self.task_table_state,
            Tab::Agents => &mut self.agent_table_state,
            Tab::Reservations => &mut self.reservation_table_state,
        }
    }

    /// Close the currently selected task (set status to Closed).
    ///
    /// # Errors
    ///
    /// Returns an error if the status update fails.
    pub async fn close_selected_task(&mut self) -> Result<()> {
        if self.active_tab != Tab::Tasks {
            return Ok(());
        }
        if let Some(idx) = self.task_table_state.selected() {
            if let Some(row) = self.tasks.get(idx) {
                let id = row.entity.id().as_str().to_string();
                self.conn
                    .update_entity_status(&id, EntityStatus::Closed)
                    .await?;
                self.refresh_tasks().await;
            }
        }
        Ok(())
    }
}
