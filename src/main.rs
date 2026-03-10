use chrono::{DateTime, Datelike};
use color_eyre::eyre::{Context, ContextCompat, OptionExt};
use gix::{Repository, objs::tree::EntryKind, prelude::*, repository::blame_file};
use std::{collections::HashMap, num::NonZeroU32, path::Path};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct YearQuarter {
    year: i32,
    quarter: u8,
}

impl TryFrom<gix::date::Time> for YearQuarter {
    type Error = color_eyre::Report;

    fn try_from(ts: gix::date::Time) -> Result<Self, Self::Error> {
        let seconds = ts.seconds;
        let datetime = DateTime::from_timestamp(seconds, 0).wrap_err("invalid timestamp")?;

        let month = datetime.month();
        let quarter = ((month - 1) / 3 + 1) as u8;

        Ok(YearQuarter {
            year: datetime.year(),
            quarter,
        })
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

    let mut layers: HashMap<YearQuarter, u32> = HashMap::new();

    for entry in head.tree()?.iter() {
        let entry = entry?;
        if entry.kind() != EntryKind::Blob {
            continue;
        }

        let path = entry.filename();

        let blame = repo.blame_file(path, head.id, blame_file::Options::default())?;

        for hunk in blame.entries {
            let commit = repo.find_commit(hunk.commit_id)?;
            let quarter = YearQuarter::try_from(commit.time()?)?;

            *layers.entry(quarter).or_default() += u32::from(hunk.len);
        }
    }

    eprintln!("{} layers:", layers.len());
    for (quarter, lines) in layers.into_iter().rev() {
        eprintln!("  {quarter:?}: {lines} lines");
    }

    Ok(())
}
