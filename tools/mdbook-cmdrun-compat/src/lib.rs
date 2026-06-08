use anyhow::{Context, Result};
use cfg_if::cfg_if;
use lazy_static::lazy_static;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::{Preprocessor, PreprocessorContext};
use regex::{Captures, Regex};
use serde::Deserialize;
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct CmdRunCompat;

lazy_static! {
    static ref CMDRUN_REG_NEWLINE: Regex = Regex::new(r"<!--[ ]*cmdrun (.*?)-->\r?\n")
        .expect("failed to compile newline cmdrun pattern");
    static ref CMDRUN_REG_INLINE: Regex = Regex::new(r"<!--[ ]*cmdrun (.*?)-->")
        .expect("failed to compile inline cmdrun pattern");
}

cfg_if! {
    if #[cfg(target_family = "unix")] {
        const LAUNCH_SHELL_COMMAND: &str = "sh";
        const LAUNCH_SHELL_FLAG: &str = "-c";
        const NEWLINE: &str = "\n";
    } else if #[cfg(target_family = "windows")] {
        const LAUNCH_SHELL_COMMAND: &str = "cmd";
        const LAUNCH_SHELL_FLAG: &str = "/C";
        const NEWLINE: &str = "\r\n";
    }
}

impl Preprocessor for CmdRunCompat {
    fn name(&self) -> &str {
        "cmdrun"
    }

    fn supports_renderer(&self, renderer: &str) -> Result<bool> {
        Ok(renderer == "html")
    }

    fn run(&self, _ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let mut result = Ok(());

        book.for_each_mut(|item| {
            if result.is_err() {
                return;
            }

            if let BookItem::Chapter(chapter) = item {
                result = Self::run_on_chapter(chapter);
            }
        });

        result.map(|_| book)
    }
}

lazy_static! {
    static ref SRC_DIR: String = get_src_dir();
}

#[derive(Deserialize)]
struct BookConfig {
    book: BookField,
}

#[derive(Deserialize)]
struct BookField {
    src: Option<String>,
}

fn get_src_dir() -> String {
    fs::read_to_string(Path::new("book.toml"))
        .map_err(|_| None::<String>)
        .and_then(|contents| toml::from_str::<BookConfig>(&contents).map_err(|_| None))
        .and_then(|config| config.book.src.ok_or(None))
        .unwrap_or_else(|_| String::from("src"))
}

impl CmdRunCompat {
    fn run_on_chapter(chapter: &mut Chapter) -> Result<()> {
        let working_dir = chapter
            .path
            .as_ref()
            .and_then(|path| Path::new(SRC_DIR.as_str()).join(path).parent().map(PathBuf::from))
            .and_then(|path| path.to_str().map(String::from))
            .unwrap_or_default();

        chapter.content = Self::run_on_content(&chapter.content, &working_dir)?;
        Ok(())
    }

    fn run_on_content(content: &str, working_dir: &str) -> Result<String> {
        let mut err = None;

        let mut result = CMDRUN_REG_NEWLINE
            .replace_all(content, |caps: &Captures| {
                Self::run_cmdrun(caps[1].to_string(), working_dir, false).unwrap_or_else(|error| {
                    err = Some(error);
                    String::new()
                })
            })
            .to_string();

        if let Some(error) = err {
            return Err(error);
        }

        result = CMDRUN_REG_INLINE
            .replace_all(&result, |caps: &Captures| {
                Self::run_cmdrun(caps[1].to_string(), working_dir, true).unwrap_or_else(|error| {
                    err = Some(error);
                    String::new()
                })
            })
            .to_string();

        match err {
            None => Ok(result),
            Some(error) => Err(error),
        }
    }

    #[cfg(target_family = "windows")]
    fn format_whitespace(text: Cow<'_, str>, inline: bool) -> String {
        let text = if inline { text.trim_end() } else { text.as_ref() };

        let mut result = text.lines().collect::<Vec<_>>().join("\r\n");
        if !inline && !result.is_empty() {
            result.push_str("\r\n");
        }
        result
    }

    #[cfg(target_family = "unix")]
    fn format_whitespace(text: Cow<'_, str>, inline: bool) -> String {
        if inline {
            text.trim_end().to_string()
        } else {
            text.to_string()
        }
    }

    fn cmdrun_error_message(message: &str, command: &str) -> String {
        format!("**cmdrun error**: {} in 'cmdrun {}'", message, command)
    }

    fn run_cmdrun(command: String, working_dir: &str, inline: bool) -> Result<String> {
        let (command, correct_exit_code): (String, Option<i32>) =
            if let Some(first_word) = command.split_whitespace().next() {
                if first_word.starts_with('-') {
                    if first_word.starts_with("--") {
                        match first_word {
                            "--strict" => (
                                command
                                    .split_whitespace()
                                    .skip(1)
                                    .collect::<Vec<&str>>()
                                    .join(" "),
                                Some(0),
                            ),
                            "--expect-return-code" => {
                                if let Some(second_word) = command.split_whitespace().nth(1) {
                                    match second_word.parse::<i32>() {
                                        Ok(return_code) => (
                                            command
                                                .split_whitespace()
                                                .skip(2)
                                                .collect::<Vec<&str>>()
                                                .join(" "),
                                            Some(return_code),
                                        ),
                                        Err(_) => {
                                            return Ok(Self::cmdrun_error_message(
                                                "No return code after '--expect-return-code'",
                                                &command,
                                            ));
                                        }
                                    }
                                } else {
                                    return Ok(Self::cmdrun_error_message(
                                        "No return code after '--expect-return-code'",
                                        &command,
                                    ));
                                }
                            }
                            other_flag => {
                                return Ok(Self::cmdrun_error_message(
                                    &format!("Unrecognized cmdrun flag {}", other_flag),
                                    &command,
                                ));
                            }
                        }
                    } else {
                        let (_, exit_code) = first_word.rsplit_once('-').unwrap_or(("", "0"));
                        match exit_code.parse::<i32>() {
                            Ok(return_code) => (
                                command
                                    .split_whitespace()
                                    .skip(1)
                                    .collect::<Vec<&str>>()
                                    .join(" "),
                                Some(return_code),
                            ),
                            Err(_) => {
                                return Ok(Self::cmdrun_error_message(
                                    &format!(
                                        "Unable to interpret short-form exit code {} as a number",
                                        first_word
                                    ),
                                    &command,
                                ));
                            }
                        }
                    }
                } else {
                    (command, None)
                }
            } else {
                (command, None)
            };

        let output = Command::new(LAUNCH_SHELL_COMMAND)
            .args([LAUNCH_SHELL_FLAG, &command])
            .current_dir(working_dir)
            .output()
            .with_context(|| "failed to run shell for cmdrun")?;

        let stdout = Self::format_whitespace(String::from_utf8_lossy(&output.stdout), inline);
        match (output.status.code(), correct_exit_code) {
            (None, _) => Ok(Self::cmdrun_error_message(
                "Command was ended before completing",
                &command,
            )),
            (Some(code), Some(correct_code)) => {
                if code != correct_code {
                    Ok(format!(
                        "**cmdrun error**: '{command}' returned exit code {code} instead of {correct_code}.{0}{1}{0}{2}",
                        NEWLINE,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ))
                } else {
                    Ok(stdout)
                }
            }
            (Some(_), None) => Ok(stdout),
        }
    }
}
