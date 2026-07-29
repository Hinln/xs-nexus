#![forbid(unsafe_code)]

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Component {
    Agent,
    Cli,
    Controller,
    Relay,
}

impl Component {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "xs-agent",
            Self::Cli => "xs-cli",
            Self::Controller => "xs-controller",
            Self::Relay => "xs-relay",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaselineReport {
    component: Component,
    version: &'static str,
}

impl BaselineReport {
    #[must_use]
    pub const fn new(component: Component) -> Self {
        Self {
            component,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

impl fmt::Display for BaselineReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} status=baseline-ready version={}",
            self.component.as_str(),
            self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{BaselineReport, Component};

    #[test]
    fn report_is_stable_and_component_specific() {
        let report = BaselineReport::new(Component::Controller).to_string();
        assert_eq!(report, "xs-controller status=baseline-ready version=0.1.0");
    }

    #[test]
    fn component_names_are_unique() {
        let names = [
            Component::Agent.as_str(),
            Component::Cli.as_str(),
            Component::Controller.as_str(),
            Component::Relay.as_str(),
        ];
        assert_eq!(names.len(), 4);
        assert_eq!(
            names
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
    }
}
