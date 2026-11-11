//! Source-owned contracts, assembled in their original matching order.
mod choreography;
mod continuous;
mod contracts;
mod echo;
mod echo_push;
mod freshness;
mod intents;
mod radar;
mod reproducer;
mod reproducer_docker;
mod reproducer_source;
mod rust_dependencies;
mod sharing;
mod storyboard;

pub(super) const ROWS: &[(&str, &str, &str)] =
    &crate::repo::messages::workflow::ordered::<_, 163>(&[
        choreography::ROWS,
        continuous::ROWS,
        contracts::ROWS,
        echo::ROWS,
        echo_push::ROWS,
        freshness::ROWS,
        intents::ROWS,
        radar::ROWS,
        reproducer::ROWS,
        reproducer_docker::ROWS,
        reproducer_source::ROWS,
        rust_dependencies::ROWS,
        sharing::ROWS,
        storyboard::ROWS,
    ]);
