use chrono::{DateTime, Datelike};
use color_eyre::eyre::{Context, ContextCompat, OptionExt};
use gix::{Repository, objs::tree::EntryKind, prelude::*, repository::blame_file};
use std::{collections::HashMap, num::NonZeroU32, path::Path};

type Layers = HashMap<YearQuarter, u32>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct YearQuarter {
    year: i32,
    quarter: u8,
}

impl From<gix::date::Time> for YearQuarter {
    fn from(ts: gix::date::Time) -> Self {
        let seconds = ts.seconds;
        let datetime = DateTime::from_timestamp(seconds, 0).expect("valid timestamp");

        let month = datetime.month();
        let quarter = ((month - 1) / 3 + 1) as u8;

        YearQuarter {
            year: datetime.year(),
            quarter,
        }
    }
}

fn open_repo(repo_path: impl AsRef<Path>) -> color_eyre::Result<Repository> {
    let mut repo = gix::discover(repo_path)?.with_object_memory(); // TODO: check if it has impact
    repo.object_cache_size(32 * 1024); // TODO: figure out how to choose!
    eprintln!(
        "opened repo at {}",
        repo.workdir().unwrap_or_else(|| repo.git_dir()).display()
    );
    Ok(repo)
}

fn main() -> color_eyre::Result<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .display_location_section(cfg!(debug_assertions))
        .install()?;

    let repo_path = std::env::args().nth(1).ok_or_eyre("must provide path")?;
    let repo = open_repo(repo_path)?;

    let head = repo
        .head()?
        .peel_to_commit()
        .wrap_err("repo doesn't have any commits")?;

    let mut series: Vec<(gix::date::Time, Layers)> = Vec::new();

    for entry in head.tree()?.iter() {
        let entry = entry?;
        if entry.kind() != EntryKind::Blob {
            continue;
        }

        let path = entry.filename();

        let blame = repo.blame_file(path, head.id, blame_file::Options::default())?;

        let mut layers = Layers::new();

        for hunk in blame.entries {
            let commit_time = repo.find_commit(hunk.commit_id).unwrap().time().unwrap();
            let quarter = YearQuarter::from(commit_time);
            *layers.entry(quarter).or_default() += u32::from(hunk.len);
        }

        series.push((head.time().unwrap(), layers));
    }

    eprintln!("series: {:#?}", series);

    Ok(())
}
