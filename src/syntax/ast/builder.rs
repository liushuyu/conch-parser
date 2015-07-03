//! Defines an interfaces to receive parse data and construct ASTs.
//!
//! This allows the parser to remain agnostic of the required source
//! representation, and frees up the library user to substitute their own.
//! If one does not require a custom AST representation, this module offers
//! a reasonable default builder implementation.
//!
//! If a custom AST representation is required you will need to implement
//! the `Builder` trait for your AST. Otherwise check out the `CommandBuilder`
//! trait if you wish to selectively overwrite several of the default
//! implementations and/or return a custom error from them.

use std::error::Error;
use syntax::ast::{self, Command, CompoundCommand, SimpleCommand, Redirect, Word};

/// An indicator to the builder of how complete commands are separated.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SeparatorKind {
    /// A semicolon appears between commands, normally indicating a sequence.
    Semi,
    /// An ampersand appears between commands, normally indicating an asyncronous job.
    Amp,
    /// A newline (and possibly a comment) appears at the end of a command before the next.
    Newline(ast::Newline),
    /// The command was delimited by a token (e.g. a compound command delimiter) or
    /// the end of input, but is *not* followed by another sequential command.
    Other,
}

/// An indicator to the builder whether an `AND` or `OR` command was parsed.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AndOrKind {
    /// An `AND` command was parsed, normally indicating the second should run if the first succeeds.
    /// Corresponds to the `&&` command separator.
    And,
    /// An `OR` command was parsed, normally indicating the second should run if the first fails.
    /// Corresponds to the `||` command separator.
    Or,
}

/// An indicator to the builder whether a `while` or `until` command was parsed.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LoopKind {
    /// A `while` command was parsed, normally indicating the loop's body should be run
    /// while the guard's exit status is successful.
    While,
    /// An `until` command was parsed, normally indicating the loop's body should be run
    /// until the guard's exit status becomes successful.
    Until,
}

/// A `Builder` implementation which builds shell commands using the AST definitions in the `ast` module.
pub struct DefaultBuilder;

/// A trait which defines an interface which the parser defined in the `parse` module
/// uses to delegate Abstract Syntax Tree creation. The methods defined here correspond
/// to their respectively named methods on the parser, and accept the relevant data for
/// each shell command type.
pub trait Builder {
    /// The type which represents the different shell commands.
    type Output;
    /// An error type that the builder may want to return.
    type Err: Error;

    /// Invoked once a complete command is found. That is, a command delimited by a
    /// newline, semicolon, ampersand, or the end of input.
    ///
    /// # Arguments
    /// * pre_cmd_comments: any comments that appear before the start of the command
    /// * cmd: the command itself, previously generated by the same builder
    /// * separator: indicates how the command was delimited
    /// * post_cmd_comments: any comments that appear after the end of the command
    fn complete_command(&mut self,
                        pre_cmd_comments: Vec<ast::Newline>,
                        cmd: Self::Output,
                        separator: SeparatorKind,
                        pos_cmd_comments: Vec<ast::Newline>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked once two pipeline commands are parsed, which are separated by '&&' or '||'.
    /// Typically the second command is run based on the exit status of the first, running
    /// if the first succeeds for an AND command, or if the first fails for an OR command.
    ///
    /// # Arguments
    /// * first: the command on the left side of the separator
    /// * kind: the type of command parsed, AND or OR
    /// * post_separator_comments: comments appearing between the AND/OR separator and the
    /// start of the second command
    /// * second: the command on the right side of the separator
    fn and_or(&mut self,
              first: Self::Output,
              kind: AndOrKind,
              post_separator_comments: Vec<ast::Newline>,
              second: Self::Output)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a pipeline of commands is parsed.
    /// A pipeline is one or more commands where the standard output of the previous
    /// typically becomes the standard input of the next.
    ///
    /// # Arguments
    /// * bang: the presence of a `!` at the start of the pipeline, typically indicating
    /// that the pipeline's exit status should be logically inverted.
    /// * cmds: a collection of tuples which are any comments appearing after a pipe token, followed
    /// by the command itself, all in the order they were parsed
    fn pipeline(&mut self,
                bang: bool,
                cmds: Vec<(Vec<ast::Newline>, Self::Output)>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when the "simplest" possible command is parsed: an executable with arguments.
    ///
    /// # Arguments
    /// * env_vars: environment variables to be defined only for the command before it is run.
    /// * cmd: the name of the command to be run. This value is optional since the shell grammar
    /// permits that a simple command be made up of only env var definitions or redirects (or both).
    /// * args: arguments to the command
    /// * redirects: redirection of any file descriptors to/from other file descriptors or files.
    fn simple_command(&mut self,
                      env_vars: Vec<(String, Option<Word>)>,
                      cmd: Option<Word>,
                      args: Vec<Word>,
                      redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a non-zero number of commands were parsed between balanced curly braces.
    /// Typically these commands should run within the current shell environment.
    ///
    /// # Arguments
    /// * cmds: the commands that were parsed between braces
    /// * redirects: any redirects to be applied over the **entire** group of commands
    fn brace_group(&mut self,
                   cmds: Vec<Self::Output>,
                   redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a non-zero number of commands were parsed between balanced parentheses.
    /// Typically these commands should run within their own environment without affecting
    /// the shell's global environment.
    ///
    /// # Arguments
    /// * cmds: the commands that were parsed between parens
    /// * redirects: any redirects to be applied over the **entire** group of commands
    fn subshell(&mut self,
                cmds: Vec<Self::Output>,
                redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a loop command like `while` or `until` is parsed.
    /// Typically these commands will execute their body based on the exit status of their guard.
    ///
    /// # Arguments
    /// * kind: the type of the loop: `while` or `until`
    /// * guard: commands that determine how long the loop will run for
    /// * body: commands to be run every iteration of the loop
    /// * redirects: any redirects to be applied over **all** commands part of the loop
    fn loop_command(&mut self,
                    kind: LoopKind,
                    guard: Vec<Self::Output>,
                    body: Vec<Self::Output>,
                    redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when an `if` conditional command is parsed.
    /// Typically an `if` command is made up of one or more guard-body pairs, where the body
    /// of the first successful corresponding guard is executed. There can also be an optional
    /// `else` part to be run if no guard is successful.
    ///
    /// # Arguments
    /// * branches: a collection of (guard, body) command groups
    /// * else_part: optional group of commands to be run if no guard exited successfully
    /// * redirects: any redirects to be applied over **all** commands within the `if` command
    fn if_command(&mut self,
                  branches: Vec<(Vec<Self::Output>, Vec<Self::Output>)>,
                  else_part: Option<Vec<Self::Output>>,
                  redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a `for` command is parsed.
    /// Typically a `for` command binds a variable to each member in a group of words and
    /// invokes its body with that variable present in the environment. If no words are
    /// specified, the command will iterate over the arguments to the script or enclosing function.
    ///
    /// # Arguments
    /// * var: the name of the variable to which each of the words will be bound
    /// * post_var_comments: any comments that appear after the variable declaration
    /// * in_words: a group of words to iterate over if present
    /// * post_word_comments: any comments that appear after the `in_words` declaration (if it exists)
    /// * body: the body to be invoked for every iteration
    /// * redirects: any redirects to be applied over **all** commands within the `for` command
    fn for_command(&mut self,
                   var: String,
                   post_var_comments: Vec<ast::Newline>,
                   in_words: Option<Vec<Word>>,
                   post_word_comments: Option<Vec<ast::Newline>>,
                   body: Vec<Self::Output>,
                   redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a `case` command is parsed.
    /// Typically this command will execute certain commands when a given word matches a pattern.
    ///
    /// # Arguments
    /// * word: the word to be matched against
    /// * post_word_comments: the comments appearing after the word to match but before the `in` reserved word
    /// * branches: the various alternatives that the `case` command can take. The first part of the tuple
    /// is a list of alternative patterns, while the second is the group of commands to be run in case
    /// any of the alternative patterns is matched. The patterns are wrapped in an internal tuple which
    /// holds all comments appearing before and after the pattern (but before the command start).
    /// * post_branch_comments: the comments appearing after the last arm but before the `esac` reserved word
    /// * redirects: any redirects to be applied over **all** commands part of the `case` block
    fn case_command(&mut self,
                    word: Word,
                    post_word_comments: Vec<ast::Newline>,
                    branches: Vec<( (Vec<ast::Newline>, Vec<Word>, Vec<ast::Newline>), Vec<Self::Output>)>,
                    post_branch_comments: Vec<ast::Newline>,
                    redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when a function declaration is parsed.
    /// Typically a function declaration overwrites any previously defined function
    /// within the current environment.
    ///
    /// # Arguments
    /// * name: the name of the function to be created
    /// * body: commands to be run when the function is invoked
    fn function_declaration(&mut self,
                            name: String,
                            body: Self::Output)
        -> Result<Self::Output, Self::Err>;

    /// Invoked when only comments are parsed with no commands following.
    /// This can occur if an entire shell script is commented out or if there
    /// are comments present at the end of the script.
    ///
    /// # Arguments
    /// * comments: the parsed comments
    fn comments(&mut self,
                comments: Vec<ast::Newline>)
        -> Result<(), Self::Err>;
}

/// A default implementation of the `Builder` trait. It allows for selectively
/// overwriting a given method without having to reimplement the rest (as long
/// as the output of the builder remains the `ast::Command` type, that is).
///
/// This implementation does not return any errors, which makes it possible
/// for an implementor of this trait to perform checks while constructing the
/// AST and return their own error type.
///
/// This implementation ignores all comments.
///
/// For more indepth documentation of each method and it's arguments, see the
/// definition of the `Builder` trait.
pub trait CommandBuilder {
    /// An error type that an implementor of this trait may return.
    type Err: Error;

    /// Constructs a `Command::Job` node with the provided inputs if the command
    /// was delimited by an ampersand or the command itself otherwise.
    fn complete_command(&mut self,
                        _pre_cmd_comments: Vec<ast::Newline>,
                        cmd: Command,
                        separator: SeparatorKind,
                        _pos_cmd_comments: Vec<ast::Newline>)
        -> Result<Command, Self::Err>
    {
        match separator {
            SeparatorKind::Semi  |
            SeparatorKind::Other |
            SeparatorKind::Newline(_) => Ok(cmd),
            SeparatorKind::Amp => Ok(Command::Job(Box::new(cmd))),
        }
    }

    /// Constructs a `Command::And` or `Command::Or` node with the provided inputs.
    fn and_or(&mut self,
              first: Command,
              kind: AndOrKind,
              _post_separator_comments: Vec<ast::Newline>,
              second: Command)
        -> Result<Command, Self::Err>
    {
        match kind {
            AndOrKind::And => Ok(Command::And(Box::new(first), Box::new(second))),
            AndOrKind::Or  => Ok(Command::Or(Box::new(first), Box::new(second))),
        }
    }

    /// Constructs a `Command::Pipe` node with the provided inputs or a `Command::Simple`
    /// node if only a single command with no status inversion is supplied.
    fn pipeline(&mut self,
                bang: bool,
                cmds: Vec<(Vec<ast::Newline>, Command)>)
        -> Result<Command, Self::Err>
    {
        debug_assert_eq!(cmds.is_empty(), false);
        let mut cmds: Vec<Command> = cmds.into_iter().map(|(_, c)| c).collect();

        // Command::Pipe is the only AST node which allows for a status
        // negation, so we are forced to use it even if we have a single
        // command. Otherwise there is no need to wrap it further.
        if bang || cmds.len() > 1 {
            cmds.shrink_to_fit();
            Ok(Command::Pipe(bang, cmds))
        } else {
            Ok(cmds.pop().unwrap())
        }
    }

    /// Constructs a `Command::Simple` node with the provided inputs.
    fn simple_command(&mut self,
                      mut env_vars: Vec<(String, Option<Word>)>,
                      cmd: Option<Word>,
                      mut args: Vec<Word>,
                      mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        env_vars.shrink_to_fit();
        args.shrink_to_fit();
        redirects.shrink_to_fit();

        Ok(Command::Simple(Box::new(SimpleCommand {
            cmd: cmd,
            vars: env_vars,
            args: args,
            io: redirects,
        })))
    }

    /// Constructs a `Command::Compound(Brace)` node with the provided inputs.
    fn brace_group(&mut self,
                   mut cmds: Vec<Command>,
                   mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        cmds.shrink_to_fit();
        redirects.shrink_to_fit();
        Ok(Command::Compound(Box::new(CompoundCommand::Brace(cmds)), redirects))
    }

    /// Constructs a `Command::Compound(Subshell)` node with the provided inputs.
    fn subshell(&mut self,
                mut cmds: Vec<Command>,
                mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        cmds.shrink_to_fit();
        redirects.shrink_to_fit();
        Ok(Command::Compound(Box::new(CompoundCommand::Subshell(cmds)), redirects))
    }

    /// Constructs a `Command::Compound(Loop)` node with the provided inputs.
    fn loop_command(&mut self,
                    kind: LoopKind,
                    mut guard: Vec<Command>,
                    mut body: Vec<Command>,
                    mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        guard.shrink_to_fit();
        body.shrink_to_fit();
        redirects.shrink_to_fit();

        let loop_cmd = match kind {
            LoopKind::While => CompoundCommand::While(guard, body),
            LoopKind::Until => CompoundCommand::Until(guard, body),
        };

        Ok(Command::Compound(Box::new(loop_cmd), redirects))
    }

    /// Constructs a `Command::Compound(If)` node with the provided inputs.
    fn if_command(&mut self,
                  mut branches: Vec<(Vec<Command>, Vec<Command>)>,
                  mut else_part: Option<Vec<Command>>,
                  mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        for &mut (ref mut guard, ref mut body) in branches.iter_mut() {
            guard.shrink_to_fit();
            body.shrink_to_fit();
        }

        for els in else_part.iter_mut() { els.shrink_to_fit(); }
        redirects.shrink_to_fit();

        Ok(Command::Compound(Box::new(CompoundCommand::If(branches, else_part)), redirects))
    }

    /// Constructs a `Command::Compound(For)` node with the provided inputs.
    fn for_command(&mut self,
                   var: String,
                   _post_var_comments: Vec<ast::Newline>,
                   mut in_words: Option<Vec<Word>>,
                   _post_word_comments: Option<Vec<ast::Newline>>,
                   mut body: Vec<Command>,
                   mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        for word in in_words.iter_mut() { word.shrink_to_fit(); }
        body.shrink_to_fit();
        redirects.shrink_to_fit();
        Ok(Command::Compound(Box::new(CompoundCommand::For(var, in_words, body)), redirects))
    }

    /// Constructs a `Command::Compound(Case)` node with the provided inputs.
    fn case_command(&mut self,
                    word: Word,
                    _post_word_comments: Vec<ast::Newline>,
                    branches: Vec<( (Vec<ast::Newline>, Vec<Word>, Vec<ast::Newline>), Vec<Command>)>,
                    _post_branch_comments: Vec<ast::Newline>,
                    mut redirects: Vec<Redirect>)
        -> Result<Command, Self::Err>
    {
        let branches = branches.into_iter().map(|((_, mut pats, _), mut cmds)| {
            pats.shrink_to_fit();
            cmds.shrink_to_fit();
            (pats, cmds)
        }).collect();

        redirects.shrink_to_fit();
        Ok(Command::Compound(Box::new(CompoundCommand::Case(word, branches)), redirects))
    }

    /// Constructs a `Command::Function` node with the provided inputs.
    fn function_declaration(&mut self,
                            name: String,
                            body: Command)
        -> Result<Command, Self::Err>
    {
        Ok(Command::Function(name, Box::new(body)))
    }

    /// Ignored by the builder.
    fn comments(&mut self,
                _comments: Vec<ast::Newline>)
        -> Result<(), Self::Err>
    {
        Ok(())
    }
}

impl<T: CommandBuilder> Builder for T {
    type Output = Command;
    type Err = T::Err;

    fn complete_command(&mut self,
                        pre_cmd_comments: Vec<ast::Newline>,
                        cmd: Self::Output,
                        separator: SeparatorKind,
                        post_cmd_comments: Vec<ast::Newline>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::complete_command(self, pre_cmd_comments, cmd, separator, post_cmd_comments)
    }

    fn and_or(&mut self,
              first: Self::Output,
              kind: AndOrKind,
              post_separator_comments: Vec<ast::Newline>,
              second: Self::Output)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::and_or(self, first, kind, post_separator_comments, second)
    }

    fn pipeline(&mut self,
                bang: bool,
                cmds: Vec<(Vec<ast::Newline>, Self::Output)>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::pipeline(self, bang, cmds)
    }

    fn simple_command(&mut self,
                      env_vars: Vec<(String, Option<Word>)>,
                      cmd: Option<Word>,
                      args: Vec<Word>,
                      redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::simple_command(self, env_vars, cmd, args, redirects)
    }

    fn brace_group(&mut self,
                   cmds: Vec<Self::Output>,
                   redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::brace_group(self, cmds, redirects)
    }

    fn subshell(&mut self,
                cmds: Vec<Self::Output>,
                redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::subshell(self, cmds, redirects)
    }

    fn loop_command(&mut self,
                    kind: LoopKind,
                    guard: Vec<Self::Output>,
                    body: Vec<Self::Output>,
                    redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::loop_command(self, kind, guard, body, redirects)
    }

    fn if_command(&mut self,
                  branches: Vec<(Vec<Self::Output>, Vec<Self::Output>)>,
                  else_part: Option<Vec<Self::Output>>,
                  redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::if_command(self, branches, else_part, redirects)
    }

    fn for_command(&mut self,
                   var: String,
                   post_var_comments: Vec<ast::Newline>,
                   in_words: Option<Vec<Word>>,
                   post_word_comments: Option<Vec<ast::Newline>>,
                   body: Vec<Self::Output>,
                   redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::for_command(self, var, post_var_comments, in_words, post_word_comments, body, redirects)
    }

    fn case_command(&mut self,
                    word: Word,
                    post_word_comments: Vec<ast::Newline>,
                    branches: Vec<( (Vec<ast::Newline>, Vec<Word>, Vec<ast::Newline>), Vec<Self::Output>)>,
                    post_branch_comments: Vec<ast::Newline>,
                    redirects: Vec<Redirect>)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::case_command(self, word, post_word_comments, branches, post_branch_comments, redirects)
    }

    fn function_declaration(&mut self,
                            name: String,
                            body: Self::Output)
        -> Result<Self::Output, Self::Err>
    {
        CommandBuilder::function_declaration(self, name, body)
    }

    fn comments(&mut self,
                comments: Vec<ast::Newline>)
        -> Result<(), Self::Err>
    {
        CommandBuilder::comments(self, comments)
    }
}

#[derive(Debug)]
/// A dummy error which implements the `Error` trait.
pub struct DummyError;

impl ::std::fmt::Display for DummyError {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.write_str("dummy error")
    }
}

impl Error for DummyError {
    fn description(&self) -> &str {
        "dummy error"
    }
}

impl CommandBuilder for DefaultBuilder {
    type Err = DummyError;
}

impl ::std::default::Default for DefaultBuilder {
    fn default() -> DefaultBuilder {
        DefaultBuilder
    }
}
