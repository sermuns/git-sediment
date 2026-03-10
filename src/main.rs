use color_eyre::eyre::{Context, OptionExt};
use gix::{Repository, fs::stack, objs::tree::EntryKind, repository::blame_file};
use kuva::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

type Layers = HashMap<Year, u32>;

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct Year(u16);

impl From<gix::date::Time> for Year {
    fn from(ts: gix::date::Time) -> Self {
        // FIXME: divide by seconds in year
        Self(u16::try_from(ts.seconds / (365 * 24 * 60 * 60)).unwrap())
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

fn render_stacked_area_plot(data: &[(gix::date::Time, Layers)]) {
    let unique_years: HashSet<Year> = data
        .iter()
        .flat_map(|(_, layers)| layers.keys().cloned())
        .collect();

    let mut stacked_area_plot =
        StackedAreaPlot::new().with_x(data.iter().map(|(time, _)| time.seconds as f64));

    for year in unique_years {
        stacked_area_plot = stacked_area_plot.with_series(
            data.iter()
                .map(|(_, layers)| *layers.get(&year).unwrap_or(&0)),
        )
    }

    // TODO: set title and legend
    let plots = vec![Plot::StackedArea(stacked_area_plot)];
    let layout = Layout::auto_from_plots(&plots);

    let svg = SvgBackend.render_scene(&render_multiple(plots, layout));
    std::fs::write("output.svg", svg).unwrap();
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

    let mut data: Vec<(gix::date::Time, Layers)> = Vec::new();

    // check all (currently) tracked files
    // FIXME: should check all historically tracked files, maybe loop through all commits instead??
    for entry in head.tree()?.iter() {
        let entry = entry?;

        // NOTE: need other kinds too maybe?
        if entry.kind() != EntryKind::Blob {
            continue;
        }

        let path = entry.filename();

        let blame = repo.blame_file(path, head.id, blame_file::Options::default())?;

        let mut layers = Layers::new();

        for hunk in blame.entries {
            let commit_time = repo.find_commit(hunk.commit_id).unwrap().time().unwrap();
            let year = Year::from(commit_time);
            *layers.entry(year).or_default() += u32::from(hunk.len);
        }

        data.push((head.time().unwrap(), layers));
    }

    eprintln!("{} data points", data.len());
    render_stacked_area_plot(&data);

    Ok(())
}
