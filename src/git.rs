use crate::cargo::core::GitReference;
use crate::cargo::sources::git::GitRemote;
use crate::cargo::util::config::Config;
use git2::{Repository as GitRepository, RepositoryInitOptions};
use quicli::prelude::*;
use remove_dir_all::remove_dir_all;
use std::env::current_dir;
use std::path::Path;
use std::path::PathBuf;
use tempfile::Builder;
use url::{ParseError, Url};

pub struct GitConfig {
    remote: Url,
    branch: GitReference,
}

impl GitConfig {
    /// Creates a new `GitConfig` by parsing `git` as a URL or a local path.
    pub fn new(git: &str, branch: String) -> Result<Self, failure::Error> {
        let remote = match Url::parse(git) {
            Ok(u) => u,
            Err(ParseError::RelativeUrlWithoutBase) => {
                let given_path = Path::new(git);
                let mut git_path = PathBuf::new();
                if given_path.is_relative() {
                    git_path.push(current_dir()?);
                    git_path.push(given_path);
                    if !git_path.exists() {
                        return Err(format_err!(
                            "Failed parsing git remote {:?}: path {:?} doesn't exist",
                            git,
                            &git_path
                        ));
                    }
                } else {
                    git_path.push(git)
                }
                let rel = "file://".to_string() + &git_path.to_str().unwrap_or("").to_string();
                Url::parse(&rel)?
            }
            Err(err) => return Err(format_err!("Failed parsing git remote {:?}: {}", git, err)),
        };

        Ok(GitConfig {
            remote,
            branch: GitReference::Branch(branch),
        })
    }

    /// Creates a new `GitConfig`, first with `new` and then as a GitHub `owner/repo` remote, like
    /// [hub].
    ///
    /// [hub]: https://github.com/github/hub
    pub fn new_abbr(git: &str, branch: String) -> Result<Self, failure::Error> {
        Self::new(git, branch.clone()).or_else(|e| {
            Self::new(&format!("https://github.com/{}.git", git), branch).map_err(|_| e)
        })
    }
}

pub fn create(project_dir: &PathBuf, args: GitConfig) -> Result<(), failure::Error> {
    let temp = Builder::new()
        .prefix(project_dir.to_str().unwrap_or("cargo-generate"))
        .tempdir()?;
    let config = Config::default()?;
    let remote = GitRemote::new(&args.remote);
    let (db, rev) = remote.checkout(&temp.path(), &args.branch, &config)?;

    // This clones the remote and handles all the submodules
    db.copy_to(rev, project_dir.as_path(), &config)?;
    Ok(())
}

pub fn remove_history(project_dir: &PathBuf) -> Result<(), failure::Error> {
    remove_dir_all(project_dir.join(".git")).context("Error cleaning up cloned template")?;
    Ok(())
}

pub fn init(
    project_dir: &PathBuf,
    branch: Option<String>,
) -> Result<GitRepository, failure::Error> {
    let mut opts = RepositoryInitOptions::new();
    opts.bare(false);
    if let Some(branch) = branch {
        opts.initial_head(&branch);
    }
    Ok(GitRepository::init_opts(project_dir, &opts).context("Couldn't init new repository")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitconfig_new_test() {
        // Remote HTTPS URL.
        let cfg = GitConfig::new(
            "https://github.com/ashleygwilliams/cargo-generate.git",
            "main".to_owned(),
        )
        .unwrap();

        assert_eq!(
            cfg.remote,
            Url::parse("https://github.com/ashleygwilliams/cargo-generate.git").unwrap()
        );
        assert_eq!(cfg.branch, GitReference::Branch("main".to_owned()));

        // Fails because "ashleygwilliams" is a "bad port number". Out of scope for now -- not sure
        // how common SSH URLs are at this point anyways...?
        assert!(GitConfig::new(
            "ssh://git@github.com:ashleygwilliams/cargo-generate.git",
            String::new(),
        )
        .is_err());

        // Local path doesn't exist.
        assert!(GitConfig::new("aslkdgjlaskjdglskj", String::new()).is_err());

        // Local path does exist.
        let remote = GitConfig::new("src", String::new())
            .unwrap()
            .remote
            .into_string();
        assert!(
            remote.ends_with("/src"),
            format!("remote {} ends with /src", &remote)
        );

        #[cfg(unix)]
        {
            assert!(
                remote.starts_with("file:///"),
                format!("remote {} starts with file:///", &remote)
            );
        }

        #[cfg(unix)]
        {
            // Absolute path.
            // If this fails because you cloned this repository into a non-UTF-8 directory... all
            // I can say is you probably had it comin'.
            let remote = GitConfig::new(current_dir().unwrap().to_str().unwrap(), String::new())
                .unwrap()
                .remote
                .into_string();
            assert!(
                remote.starts_with("file:///"),
                format!("remote {} starts with file:///", &remote)
            );
        }
    }

    #[test]
    fn gitconfig_new_abbr_test() {
        // Abbreviated owner/repo form
        assert_eq!(
            GitConfig::new_abbr("ashleygwilliams/cargo-generate", String::new())
                .unwrap()
                .remote,
            Url::parse("https://github.com/ashleygwilliams/cargo-generate.git").unwrap()
        );
    }
}
