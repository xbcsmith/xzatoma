# Thing to Fix

## Command Calls

Any input that starts with a '/' should be flagged as a command and should be handled accordingly. If the command does not exist return an error and display the help message. The / can be escaped with `,", or '.

## Subagents

We should be able to configure what models and provider are used for subagents. This includes the ability to specify the model, provider, and any additional parameters required for the subagent. This should be configurable through a configuration file or environment variables or flags passed to the CLI. For example if I am using the copilot provider with the gpt-5.2 model and I want to use the gpt-5-mini model for the subagent, I can specify the following configuration:

```
{
  "subagents": {
    "default": {
      "model": "gpt-5-mini",
      "provider": "copilot"
    }
  }
}
```

And if I want to use ollama for my subagents I will need to configure it in the config file.

We should not use subagents by default in chat. In chat they should be disabled by default and only enabled if the user explicitly requests it. A prompt that mentions subagents should turn on subagents. For example, "Use subagents to run a security analysis on @src/foo.rs" would use the default subagent configuration.

Complete - [Subagent Configuration](./subagent_configuration_plan.md)

## Tracking context window

As the context window fills up several things need to happen depending on the mode xzatoma is runnign in. In chat mode it needs to stop processing and warn the user the window is filling up. At that point we probably need a new summary feature that summarizes the current context, starts a new session, and adds the summary to the context window. We probably also need a command to summarize the current context and add it to the context window. So in chat mode I would see the warning message and then run the command `/context summary` which should use the default model. the command `/context summary --model gpt-5.2` should use the "gpt-5.2" model regardless of config or default model. In run mode we should do this automatically. We should be able to configure the model we use to do the summary so we do not waste premium tokens when doing the summary. This feature should be added to the config file

Complete - [Tracking Context Window](./context_window_management_plan.md)

## File Path Completion

We should be able to complete file paths in the chat prompt with TAB. This should be done by looking at the current working directory and the files in the directory. If the path is not found we should return an error.
