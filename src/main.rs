use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, OptionExt};
use gix::{Repository, objs::tree::EntryKind, repository::blame_file};
use indicatif::ProgressBar;
use kuva::prelude::*;
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

type Layers = BTreeMap<gix::date::Time, u32>;

fn open_repo(repo_path: impl AsRef<Path>) -> color_eyre::Result<Repository> {
    let mut repo = gix::discover(repo_path)?.with_object_memory();
    repo.object_cache_size(32 * 1024);
    Ok(repo)
}

fn layers_for_commit(repo: &Repository, oid: gix::ObjectId) -> (gix::date::Time, Layers) {
    let commit = repo.find_commit(oid).unwrap();
    let commit_time = commit.time().expect("commit time should be present");

    let tree = commit.tree().unwrap();

    let mut layers: Layers = BTreeMap::new();

    for entry in tree.iter() {
        let entry = entry.unwrap();
        if entry.kind() != EntryKind::Blob {
            continue;
        }
        let path = entry.filename();

        let blame = repo
            .blame_file(path, commit.id, blame_file::Options::default())
            .unwrap();

        for hunk in blame.entries {
            let blamed_commit = repo.find_commit(hunk.commit_id).unwrap();
            let blamed_time = blamed_commit.time().unwrap();
            *layers.entry(blamed_time).or_default() += u32::from(hunk.len);
        }
    }

    (commit_time, layers)
}

fn render_stacked_area_plot(data: &[(gix::date::Time, Layers)]) -> color_eyre::Result<()> {
    let times: BTreeSet<_> = data.iter().flat_map(|(_, layers)| layers.keys()).collect();

    let x: Vec<f64> = data.iter().map(|(t, _)| t.seconds as f64).collect();

    let palette = [
        "steelblue",
        "orange",
        "mediumseagreen",
        "tomato",
        "slateblue",
        "goldenrod",
        "hotpink",
        "teal",
        "peru",
        "darkseagreen",
    ];

    let mut sa = StackedAreaPlot::new().with_x(x);

    for (idx, time) in times.iter().enumerate() {
        let series: Vec<f64> = data
            .iter()
            .map(|(_, layers)| layers.get(time).copied().unwrap_or(0) as f64)
            .collect();

        let color = palette[idx % palette.len()];
        sa = sa
            .with_series(series)
            .with_color(color)
            .with_legend(format!(
                "{}",
                DateTime::<Utc>::from_timestamp(time.seconds, 0).unwrap()
            ));
    }

    let plots = vec![Plot::StackedArea(sa)];
    let layout = Layout::auto_from_plots(&plots)
        .with_title("Surviving LoC by year of last change")
        .with_x_label("Commit time (unix seconds)")
        .with_y_label("Lines of code");

    let svg = SvgBackend.render_scene(&render_multiple(plots, layout));
    std::fs::write("output.svg", svg).wrap_err("failed to write output.svg")?;

    Ok(())
}

fn main() -> color_eyre::Result<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .display_location_section(cfg!(debug_assertions))
        .install()?;

    let repo_path = std::env::args().nth(1).ok_or_eyre("must provide path")?;
    let repo = open_repo(&repo_path)?;
    eprintln!(
        "opened repo at {}",
        repo.workdir().unwrap_or_else(|| repo.git_dir()).display()
    );

    let head_id = repo
        .head()?
        .try_into_peeled_id()?
        .ok_or_eyre("repo doesn't have any commits")?;

    let commit_ids: Vec<_> = head_id
        .ancestors()
        .all()?
        .map_while(Result::ok)
        .map(|c| c.id().detach())
        .collect();

    let num_commits = commit_ids.len();
    eprintln!("found {} commits", num_commits);

    let pb = ProgressBar::new(num_commits as u64);

    let data: Vec<_> = commit_ids
        .into_par_iter()
        .map(|oid| {
            let repo = open_repo(&repo_path).unwrap();
            let layers = layers_for_commit(&repo, oid);
            pb.inc(1);
            layers
        })
        .collect();

    eprintln!("{} time points", data.len());
    render_stacked_area_plot(&data)?;

    Ok(())
}
