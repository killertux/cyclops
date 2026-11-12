# Logs

Back to the [specification index](../README.md).

Every analysis writes what it did to `.cyclops/logs/cyclops.log`, so that when something
goes wrong, such as an indexer that fails, the whole story can be read. The log says
when each analysis started and finished and how long each step took. For
[precise analysis](precise-analysis.md) it says each indexer it ran: the command, the
folder it ran in, how it ended, and everything it printed.

The log grows at its end. Once it is larger than 10 MB, it is moved to `cyclops.log.1`,
in place of the one before, and a new log starts. Like the cache, it is
[kept out of git](../project/initializing.md#what-git-sees).

[//]: # "import Spec.Fixtures"
[//]: # "decisionTable Core"

| # situation       | workspace | steps                                         | log text                | log includes? |
| ----------------- | --------- | --------------------------------------------- | ----------------------- | ------------- |
| An analysis       | rust-shop |                                               | analysis finished       | yes           |
| An indexer it ran | rust-shop | `precise analysis`                            | rust-analyzer scip      | yes           |
| What it printed   | rust-shop | `precise analysis`                            | Generating SCIP         | yes           |
| Not asked for     | rust-shop |                                               | rust-analyzer           | no            |
| A missing tool    | rust-shop | `remove tool rust-analyzer; precise analysis` | rust-analyzer not found | yes           |
