# Deep-review finding 3: syntax failures honor the S1-B12 machine
# contract — JSON with a category on stderr — instead of clap prose.
# Help and version remain human-facing text on stdout (S1-B10 relies on
# `--help` succeeding).
Feature: Syntax failures speak the machine contract

  Scenario: An unknown command fails as JSON validation
    When the CLI is invoked with arguments "frobnicate"
    Then the command fails with error category "validation"

  Scenario: A missing required flag fails as JSON validation
    When the CLI is invoked with arguments "create"
    Then the command fails with error category "validation"

  Scenario: An unexpected flag fails as JSON validation
    When the CLI is invoked with arguments "list --frobnicate"
    Then the command fails with error category "validation"
