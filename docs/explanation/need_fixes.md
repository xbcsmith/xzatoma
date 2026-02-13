# Thing to Fix


## Command Calls

Any input that starts with a '/' should be flagged as a command and should be handled accordingly. If the command does not exist return an error and display the help message. The / can be escaped with `,", or '.

## Tracking context window

As the context window fills up several things need to happen depending on the mode xzatoma is runnign in. In chat mode it needs to stop processing and warn the user the window is filling up. At that point we probably need a new summary feature that summarizes the current context, starts a new session and adds the summary to the context window. We probably also need a command to summarize the current context and add it to the context window. So in chat mode I would see the warning message and then run the command `/context summary`. In run mode we should do this automatically. We should be able to configure the model we use to do the summary so we do not waste premium tokens when doing the summary.

## File Path Completion

We should be able to complete file paths in the prompt with TAB. This should be done by looking at the current working directory and the files in the directory. If the path is not found we should return an error.
