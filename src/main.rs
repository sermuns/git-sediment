use color_eyre::eyre::{Context, OptionExt};
use gix::{Repository, prelude::*, repository::blame_file};
use std::{collections::HashMap, path::Path};

fn open_repo(repo_path: impl AsRef<Path>) -> color_eyre::Result<Repository> {
    let mut repo = gix::discover(repo_path)?;
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

    for info in head.ancestors().all()? {
        let info = info?;
        let commit = info.object()?;
        for entry in commit.tree()?.iter() {
            let entry = entry?;
            let filename = entry.filename();
            let blame = repo.blame_file(filename, commit.id, blame_file::Options::default())?;
            eprintln!("{}: {} blame entries", filename, blame.entries.len());
        }
    }

    Ok(())
}
