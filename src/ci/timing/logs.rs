//! A job's whole log as the forge keeps it (GitHub Actions), read through `gh`. Nothing is
//! re-run; an expired, unwritten or oversized log is an error, which callers report as unknown.

use super::*;

/// The endpoint of job `job`'s log.
pub(crate) fn endpoint(project: &str, job: u64) -> Result<String, String> {
    if job == 0 {
        return Err("the job has no recorded id".into());
    }
    Ok(format!("repos/{project}/actions/jobs/{job}/logs"))
}

/// The log of job `job` of `project`.
pub fn job_log(repo: &Path, project: &Project, job: u64) -> Result<String, String> {
    if project.forge != Forge::GitHub {
        return Err("Job logs are read from GitHub Actions only.".into());
    }
    gh_api(repo, project, &endpoint(&project.path, job)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_log_is_read_by_its_exact_job_id() {
        assert_eq!(
            endpoint("o/r", 42),
            Ok("repos/o/r/actions/jobs/42/logs".to_owned())
        );
        assert!(endpoint("o/r", 0).is_err());
        let gitlab = Project {
            forge: Forge::GitLab,
            host: "gitlab.com".into(),
            path: "o/r".into(),
        };
        let here = std::path::Path::new(".");
        assert!(job_log(here, &gitlab, 42).is_err_and(|e| e.contains("GitHub")));
    }
}
