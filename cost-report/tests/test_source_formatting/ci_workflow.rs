//! A reader of the workflows' text: a workflow's jobs, a job's lines and steps, the text of each
//! `run:`, and the simple commands a line holds. Line-based, as the workflows are written; enough of
//! bash to find what each command runs, not a parser of either language.

use crate::ci_coverage::fold_continuations;

/// A word before a command's own: a keyword, or a variable's assignment.
pub(crate) fn is_keyword_or_assignment(word: &str) -> bool {
    let assignment = word.split_once('=').is_some_and(|(name, _)| {
        let name = name.trim_end_matches('+');
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    assignment
        || matches!(
            word,
            "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "!" | "{" | "}" | "time"
        )
}

/// The job's lines, from its header to the next job's, comments included.
pub(crate) fn job_lines<'a>(workflow: &'a str, job: &str) -> Option<Vec<&'a str>> {
    let header = format!("  {job}:");
    let mut lines = workflow.lines().skip_while(|l| l.trim_end() != header);
    lines.next()?;
    Some(
        lines
            .take_while(|l| {
                let body = l.trim_start();
                body.is_empty() || body.starts_with('#') || l.len() - body.len() > 2
            })
            .collect(),
    )
}

/// The text of each `run:` of a job: an inline value, or a block scalar, dedented.
pub(crate) fn run_scripts(lines: &[&str]) -> Vec<String> {
    let mut scripts = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let body = line.trim_start().trim_start_matches("- ");
        let Some(value) = body.strip_prefix("run:") else {
            continue;
        };
        let value = value.trim();
        if !value.starts_with(['|', '>']) {
            scripts.push(value.to_string());
            continue;
        }
        let key_column = line.len() - body.len();
        let mut block = Vec::new();
        while let Some(l) = lines.get(i) {
            if !l.trim().is_empty() && l.len() - l.trim_start().len() <= key_column {
                break;
            }
            block.push(l.trim_start());
            i += 1;
        }
        scripts.push(block.join("\n"));
    }
    scripts
}

/// The line with each `$(…)` replaced by a word, then each substitution's own text: a command
/// inside one runs where the line runs.
pub(crate) fn substitutions(line: &str) -> Vec<String> {
    let mut outer = String::new();
    let mut inner = Vec::new();
    let mut chars = line.char_indices().peekable();
    let mut quoted = false;
    while let Some((at, c)) = chars.next() {
        if c == '\'' {
            quoted = !quoted;
        }
        if quoted || c != '$' || chars.peek().map(|(_, n)| *n) != Some('(') {
            outer.push(c);
            continue;
        }
        chars.next();
        let mut depth = 1;
        let start = at + 2;
        let mut end = line.len();
        for (i, n) in chars.by_ref() {
            depth += match n {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            if depth == 0 {
                end = i;
                break;
            }
        }
        outer.push('_');
        inner.extend(substitutions(&line[start..end]));
    }
    let mut all = vec![outer];
    all.extend(inner);
    all
}

/// The simple commands of a line, each its words up to its first redirection, quotes kept.
pub(crate) fn simple_commands(line: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    // After a redirection the words are its target and what the runner's shell does with it.
    let mut redirected = false;
    let mut chars = line.chars().peekable();
    let end_word = |word: &mut String, words: &mut Vec<String>, redirected: bool| {
        if !word.is_empty() && !redirected {
            words.push(std::mem::take(word));
        }
        word.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                word.push(c);
                for n in chars.by_ref() {
                    word.push(n);
                    if n == c {
                        break;
                    }
                }
            }
            '\\' => {
                word.push(c);
                word.extend(chars.next());
            }
            '#' if word.is_empty() => break,
            // An array assignment, `files=(a b)`, is one word: its elements run nothing.
            '(' if word.ends_with('=') => {
                word.push(c);
                for n in chars.by_ref() {
                    word.push(n);
                    if n == ')' {
                        break;
                    }
                }
            }
            ';' | '|' | '&' | '(' | ')' => {
                end_word(&mut word, &mut words, redirected);
                commands.push(std::mem::take(&mut words));
                redirected = false;
            }
            '<' | '>' => {
                if !word.chars().all(|d| d.is_ascii_digit()) {
                    end_word(&mut word, &mut words, redirected);
                }
                word.clear();
                while chars.next_if(|n| matches!(n, '>' | '&')).is_some() {}
                redirected = true;
            }
            c if c.is_whitespace() => end_word(&mut word, &mut words, redirected),
            c => word.push(c),
        }
    }
    end_word(&mut word, &mut words, redirected);
    commands.push(words);
    commands.retain(|w| !w.is_empty());
    commands
}

/// The steps of a job, each its lines from its `- ` to the next step's, comments included.
pub(crate) fn job_steps<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let Some(at) = lines.iter().position(|l| l.trim() == "steps:") else {
        return Vec::new();
    };
    let body = &lines[at + 1..];
    let Some(indent) = body
        .iter()
        .find(|l| l.trim_start().starts_with("- "))
        .map(|l| l.len() - l.trim_start().len())
    else {
        return Vec::new();
    };
    let mut steps: Vec<Vec<&str>> = Vec::new();
    for line in body {
        let at_step =
            line.len() - line.trim_start().len() == indent && line.trim_start().starts_with("- ");
        if at_step {
            steps.push(Vec::new());
        }
        if let Some(step) = steps.last_mut() {
            step.push(line);
        }
    }
    steps
}

/// The jobs of a workflow: the keys under its top-level `jobs:`.
pub(crate) fn workflow_jobs(workflow: &str) -> Vec<&str> {
    workflow
        .lines()
        .skip_while(|l| l.trim_end() != "jobs:")
        .skip(1)
        .take_while(|l| l.is_empty() || l.starts_with([' ', '#']))
        .filter_map(|l| l.strip_prefix("  "))
        .filter(|l| !l.starts_with([' ', '#']))
        .filter_map(|l| l.trim_end().strip_suffix(':'))
        .collect()
}

/// A `run:` script's logical lines, trimmed: a line continued by a backslash, or an array assignment
/// left open, `files=(a` to its `)`, is one.
pub(crate) fn logical_lines(script: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for line in fold_continuations(script) {
        let line = line.trim();
        if open {
            let last = out.last_mut().expect("an open array has its line");
            last.push(' ');
            last.push_str(line);
        } else {
            out.push(line.to_string());
        }
        let last = out.last().expect("a line was pushed");
        open = last.contains("=(") && last.matches('(').count() > last.matches(')').count();
    }
    out
}

/// The words that run the word after them, each with those of its options that take a value.
const PREFIX_WORDS: &[(&str, &[&str])] = &[
    ("timeout", &["-k", "-s", "--kill-after", "--signal"]),
    ("env", &["-u", "-C", "-S", "--unset", "--chdir", "--split-string"]),
    ("exec", &["-a"]),
    ("nice", &["-n", "--adjustment"]),
    ("nohup", &[]),
    ("command", &[]),
    ("sudo", &["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-T", "-U"]),
    ("xargs", &["-a", "-d", "-E", "-I", "-L", "-n", "-P", "-s"]),
];

/// Every command a line runs, each its words from the command word on, quotes kept: those of
/// each `$(…)` too, past the prefix words that run the word after them, and through a shell, its
/// `-c` text read as a line, or its script.
pub(crate) fn commands(line: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for part in substitutions(line) {
        for words in simple_commands(&part) {
            let words = past_prefix_words(&words);
            match words.first().map(|w| unquoted(w)) {
                Some(shell) if shell == "bash" || shell == "sh" => {
                    out.extend(shell_commands(&words[1..]))
                }
                Some(_) => out.push(words.to_vec()),
                None => {}
            }
        }
    }
    out
}

/// The words from the command word on: assignments, keywords and prefix words skipped, each
/// prefix word with its options and `timeout` with its duration. `command -v` runs nothing.
fn past_prefix_words(words: &[String]) -> &[String] {
    let mut rest = words;
    loop {
        while rest.first().is_some_and(|w| is_keyword_or_assignment(w)) {
            rest = &rest[1..];
        }
        let Some((word, valued)) = rest
            .first()
            .and_then(|w| PREFIX_WORDS.iter().find(|(p, _)| p == w))
        else {
            return rest;
        };
        rest = &rest[1..];
        while let Some(option) = rest.first().filter(|w| w.starts_with('-')) {
            rest = &rest[1..];
            if option == "--" {
                break;
            }
            if *word == "command" && (option == "-v" || option == "-V") {
                return &[];
            }
            if valued.contains(&option.as_str()) {
                rest = rest.get(1..).unwrap_or_default();
            }
        }
        if *word == "timeout" {
            rest = rest.get(1..).unwrap_or_default();
        }
    }
}

/// What a shell runs, given the words after `bash` or `sh`: its `-c` text, read as a line, or its
/// script with the script's words. One reading stdin runs nothing a line shows.
fn shell_commands(args: &[String]) -> Vec<Vec<String>> {
    let mut i = 0;
    while let Some(option) = args.get(i).filter(|w| w.starts_with(['-', '+'])) {
        i += 1;
        // A long option (--norc, --login) takes no value; in a short group, -o takes the next word.
        if option.starts_with("--") {
            continue;
        }
        if option.contains('o') {
            i += 1;
        }
        if option.contains('c') {
            return args
                .get(i)
                .map(|text| commands(&unquoted(text)))
                .unwrap_or_default();
        }
    }
    match args.get(i..) {
        Some(script) if !script.is_empty() => vec![script.to_vec()],
        _ => Vec::new(),
    }
}

/// A word as the shell passes it on: its quotes taken off, and a backslash's escape.
pub(crate) fn unquoted(word: &str) -> String {
    let mut out = String::new();
    let mut quote = None;
    let mut chars = word.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '\\') => out.extend(chars.next()),
            (Some('"'), '\\') if chars.peek().is_some_and(|n| "$`\"\\".contains(*n)) => {
                out.extend(chars.next())
            }
            _ => out.push(c),
        }
    }
    out
}
