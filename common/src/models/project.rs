#[cfg(feature = "openapi")]
use crate::ulid_type;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS,
    presets::{NOTHING, UTF8_FULL},
    Attribute, Cell, CellAlignment, Color, ContentArrangement, Table,
};
use crossterm::style::Stylize;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use strum::EnumString;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Timeframe before a project is considered idle
pub const DEFAULT_IDLE_MINUTES: u64 = 30;

/// Function to set [DEFAULT_IDLE_MINUTES] as a serde default
pub const fn default_idle_minutes() -> u64 {
    DEFAULT_IDLE_MINUTES
}

#[derive(Deserialize, Serialize, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = shuttle_common::models::project::Response))]
pub struct Response {
    #[cfg_attr(feature = "openapi", schema(schema_with = ulid_type))]
    pub id: String,
    pub name: String,
    #[cfg_attr(feature = "openapi", schema(value_type = shuttle_common::models::project::State))]
    pub state: State,
    pub idle_minutes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, EnumString)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = shuttle_common::models::project::State))]
pub enum State {
    Creating { recreate_count: usize },
    Attaching { recreate_count: usize },
    Recreating { recreate_count: usize },
    Starting { restart_count: usize },
    Restarting { restart_count: usize },
    Started,
    Ready,
    Stopping,
    Stopped,
    Rebooting,
    Destroying,
    Destroyed,
    Errored { message: String },
    Deleted,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Creating { .. }, Self::Creating { .. })
                | (Self::Attaching { .. }, Self::Attaching { .. })
                | (Self::Recreating { .. }, Self::Recreating { .. })
                | (Self::Starting { .. }, Self::Starting { .. })
                | (Self::Restarting { .. }, Self::Restarting { .. })
                | (Self::Started, Self::Started)
                | (Self::Ready, Self::Ready)
                | (Self::Stopping, Self::Stopping)
                | (Self::Stopped, Self::Stopped)
                | (Self::Rebooting, Self::Rebooting)
                | (Self::Destroying, Self::Destroying)
                | (Self::Destroyed, Self::Destroyed)
                | (Self::Errored { .. }, Self::Errored { .. })
        )
    }
}

impl Eq for State {}

impl Display for Response {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"Project "{}" is {}"#,
            self.name,
            self.state
                .to_string()
                // Unwrap is safe because Color::from_str returns the color white if the argument is not a Color.
                .with(crossterm::style::Color::from_str(self.state.get_color()).unwrap())
        )
    }
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Creating { recreate_count } => {
                if *recreate_count > 0 {
                    write!(f, "creating (attempt {})", recreate_count)
                } else {
                    write!(f, "creating")
                }
            }
            State::Attaching { recreate_count } => {
                if *recreate_count > 0 {
                    write!(f, "attaching (attempt {})", recreate_count)
                } else {
                    write!(f, "attaching")
                }
            }
            State::Recreating { recreate_count } => {
                if *recreate_count > 0 {
                    write!(f, "recreating (attempt {})", recreate_count)
                } else {
                    write!(f, "recreating")
                }
            }
            State::Starting { restart_count } => {
                if *restart_count > 0 {
                    write!(f, "starting (attempt {})", restart_count)
                } else {
                    write!(f, "starting")
                }
            }
            State::Restarting { restart_count } => {
                if *restart_count > 0 {
                    write!(f, "restarting (attempt {})", restart_count)
                } else {
                    write!(f, "restarting")
                }
            }
            State::Started => write!(f, "started"),
            State::Ready => write!(f, "ready"),
            State::Stopping => write!(f, "stopping"),
            State::Stopped => write!(f, "stopped"),
            State::Rebooting => write!(f, "rebooting"),
            State::Destroying => write!(f, "destroying"),
            State::Destroyed => write!(f, "destroyed"),
            State::Errored { message } => {
                write!(f, "errored\n\tmessage: {message}")
            }
            State::Deleted => write!(f, "deleted"),
        }
    }
}

impl State {
    /// We return a &str rather than a Color here, since `comfy-table` re-exports
    /// crossterm::style::Color and we depend on both `comfy-table` and `crossterm`
    /// we may end up with two different versions of Color.
    pub fn get_color(&self) -> &str {
        match self {
            Self::Creating { recreate_count }
            | Self::Attaching { recreate_count }
            | Self::Recreating { recreate_count }
                if recreate_count > &0usize =>
            {
                "dark_yellow"
            }
            Self::Starting { restart_count } | Self::Restarting { restart_count }
                if restart_count > &0usize =>
            {
                "dark_yellow"
            }
            Self::Creating { .. }
            | Self::Attaching { .. }
            | Self::Starting { .. }
            | Self::Started => "cyan",
            Self::Recreating { .. } | Self::Restarting { .. } | Self::Rebooting => "dark_yellow",
            Self::Ready => "green",
            Self::Stopped | Self::Stopping | Self::Destroying | Self::Destroyed => "blue",
            Self::Errored { .. } | Self::Deleted => "red",
        }
    }
}

/// Config when creating a new project
#[derive(Deserialize, Serialize)]
pub struct Config {
    pub idle_minutes: u64,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = shuttle_common::models::project::AdminResponse))]
pub struct AdminResponse {
    pub project_name: String,
    pub account_name: String,
}

pub fn get_projects_table(projects: &Vec<Response>, page: u32, raw: bool) -> String {
    if projects.is_empty() {
        // The page starts at 1 in the CLI.
        let mut s = if page <= 1 {
            "No projects are linked to this account\n".to_string()
        } else {
            "No more projects are linked to this account\n".to_string()
        };
        if !raw {
            s = s.yellow().bold().to_string();
        }

        s
    } else {
        let mut table = Table::new();

        if raw {
            table
                .load_preset(NOTHING)
                .set_content_arrangement(ContentArrangement::Disabled)
                .set_header(vec![
                    Cell::new("Project Name").set_alignment(CellAlignment::Left),
                    Cell::new("Status").set_alignment(CellAlignment::Left),
                ]);
        } else {
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::DynamicFullWidth)
                .set_header(vec![
                    Cell::new("Project Name")
                        .set_alignment(CellAlignment::Center)
                        .add_attribute(Attribute::Bold),
                    Cell::new("Status")
                        .set_alignment(CellAlignment::Center)
                        .add_attribute(Attribute::Bold),
                ]);
        }

        for project in projects.iter() {
            if raw {
                table.add_row(vec![Cell::new(&project.name), Cell::new(&project.state)]);
            } else {
                table.add_row(vec![
                    Cell::new(&project.name),
                    Cell::new(&project.state)
                        // Unwrap is safe because Color::from_str returns the color white if the argument is not a Color.
                        .fg(Color::from_str(project.state.get_color()).unwrap())
                        .set_alignment(CellAlignment::Center),
                ]);
            }
        }

        format!(
            "\nThese projects are linked to this account\n{table}\nMore projects might be available on the next page using --page.\n"
        )
    }
}
