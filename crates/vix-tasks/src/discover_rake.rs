//! Discovery of Rake tasks from a `Rakefile` and `.rake` files.

use regex::Regex;
use std::sync::LazyLock;

use crate::task::{NamedTask, TaskSource};

// `task :name`, `task "name"`, `task 'name'`, optionally `task(:name)`,
// optionally followed by `=> [:dep]` or other trailing text — only the name
// token is captured.
// The capture class deliberately includes `#{}` so an interpolated name
// (e.g. `task "release_#{ver}"`) is captured whole and then rejected by
// `record_task`'s `contains("#{")` check, rather than silently truncated at
// the `#` into a wrong-but-plausible-looking plain name.
static TASK_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^task\s*\(?\s*[:'"]([A-Za-z0-9_.:\-{}#]+)['"]?"#).unwrap());

// `namespace :name do` / `namespace "name" do`.
static NAMESPACE_OPEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^namespace\s*\(?\s*[:'"]([A-Za-z0-9_.\-]+)['"]?\s+do\b"#).unwrap()
});

/// Find Rake task definitions across `rakefiles` (`[(filename, content),
/// ...]` — the host collects `Rakefile` plus any `.rake` files it finds,
/// e.g. under `rakelib/`/`tasks/`/`lib/tasks/`). Command is
/// `rake NAME` (or `bundle exec rake NAME` when `use_bundler` is set — a
/// real implementation would set this whenever a `Gemfile` is present,
/// which this crate already knows how to check for as plain marker-file
/// presence, so it is left as a parameter rather than guessed here).
/// Prefixed `rake:`.
///
/// Regex/line-scan based, not a real Ruby parser: `namespace :foo do ... end`
/// blocks are tracked with a simple `do`/`end` depth counter to qualify
/// nested task names as `foo:bar`, which handles the common single-level
/// case but can mis-track deeply irregular formatting (e.g. a `do`/`end`
/// pair opened and closed on the same line). Tasks whose name looks
/// computed/interpolated (contains `#{`) are skipped, since a regex cannot
/// evaluate Ruby string interpolation.
#[must_use]
pub fn discover_rake_tasks(rakefiles: &[(&str, &str)], use_bundler: bool) -> Vec<NamedTask> {
    let mut out = Vec::new();
    for (_, content) in rakefiles {
        out.extend(discover_rake_tasks_in_file(content, use_bundler));
    }
    out
}

fn discover_rake_tasks_in_file(content: &str, use_bundler: bool) -> Vec<NamedTask> {
    let mut namespace_stack: Vec<(String, usize)> = Vec::new();
    let mut do_depth: usize = 0;
    let mut out = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();

        if let Some(caps) = NAMESPACE_OPEN.captures(line) {
            do_depth += 1;
            namespace_stack.push((caps[1].to_string(), do_depth));
            continue;
        }
        if let Some(caps) = TASK_DEF.captures(line) {
            record_task(&caps[1], &namespace_stack, use_bundler, &mut out);
            if ends_with_block_open(line) {
                do_depth += 1;
            }
            continue;
        }
        if line == "end" {
            if namespace_stack
                .last()
                .is_some_and(|(_, depth)| *depth == do_depth)
            {
                namespace_stack.pop();
            }
            do_depth = do_depth.saturating_sub(1);
            continue;
        }
        if ends_with_block_open(line) {
            do_depth += 1;
        }
    }
    out
}

fn ends_with_block_open(line: &str) -> bool {
    line.ends_with(" do") || line == "do"
}

fn record_task(
    name: &str,
    namespace_stack: &[(String, usize)],
    use_bundler: bool,
    out: &mut Vec<NamedTask>,
) {
    if name.contains("#{") {
        return;
    }
    let qualified = namespace_stack
        .iter()
        .map(|(n, _)| n.as_str())
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join(":");
    let bundler = if use_bundler { "bundle exec " } else { "" };
    out.push(NamedTask {
        name: format!("rake:{qualified}"),
        command: format!("{bundler}rake {qualified}"),
        source: TaskSource::Discovered,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_top_level_tasks() {
        let rakefile = "task :build do\n  sh 'cc'\nend\n\ntask :test => [:build] do\nend\n";
        let tasks = discover_rake_tasks(&[("Rakefile", rakefile)], false);
        let names: Vec<_> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["rake:build", "rake:test"]);
        assert_eq!(tasks[0].command, "rake build");
    }

    #[test]
    fn qualifies_tasks_inside_namespace() {
        let rakefile = "namespace :db do\n  task :migrate do\n    puts 'migrating'\n  end\nend\n";
        let tasks = discover_rake_tasks(&[("Rakefile", rakefile)], false);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "rake:db:migrate");
        assert_eq!(tasks[0].command, "rake db:migrate");
    }

    #[test]
    fn use_bundler_prefixes_command() {
        let rakefile = "task :test do\nend\n";
        let tasks = discover_rake_tasks(&[("Rakefile", rakefile)], true);
        assert_eq!(tasks[0].command, "bundle exec rake test");
    }

    #[test]
    fn skips_interpolated_task_names() {
        let rakefile = "task \"release_#{ver}\" do\nend\n";
        assert!(discover_rake_tasks(&[("Rakefile", rakefile)], false).is_empty());
    }

    #[test]
    fn scans_multiple_files() {
        let a = "task :a do\nend\n";
        let b = "task :b do\nend\n";
        let tasks = discover_rake_tasks(&[("Rakefile", a), ("tasks/b.rake", b)], false);
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn namespace_leaves_after_its_end() {
        let rakefile = "namespace :db do\n  task :migrate do\n  end\nend\ntask :outside do\nend\n";
        let tasks = discover_rake_tasks(&[("Rakefile", rakefile)], false);
        let names: Vec<_> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["rake:db:migrate", "rake:outside"]);
    }
}
