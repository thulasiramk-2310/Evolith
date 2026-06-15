# Alpha Feedback

## Internal Alpha Feedback #0

### Overall Impression

The underlying reconstruction appears to work, but the output is difficult to interpret without prior knowledge of Evolith's internal model.

The engine feels ahead of the user experience. Users are likely to understand the idea after explanation, but not from the raw output alone.

### Main Problem

Metrics are presented before meaning.

Current output such as:

```text
Anchor Coverage: 22.8%
Dominant Anchor Ratio: 0.56
Avg Events/Feature: 6.33
Episode Candidate
```

is technically useful to the creator, but opaque to an external developer.

### Most Confusing Areas

- Anchor Coverage
- Dominant Anchor Ratio
- Avg Events/Feature
- Episode Candidate

### Suggested Improvement

Shift human-facing output from technical metric names to user-facing explanations.

Current:

```text
Anchor Coverage: 22.8%
```

Better:

```text
Only 23% of commits contain strong identity signals.
```

Current:

```text
Dominant Anchor Ratio: 0.56
```

Better:

```text
Features have moderately consistent identities.
```

Current:

```text
Episode Candidate
```

Better:

```text
Unidentified Work Stream
```

or:

```text
Needs More Signals
```

### Preferred User-Facing Shape

```text
Repository Health: Moderate

Why?
- Only 23% of commits contain identifiable anchors.
- 4 of 9 reconstructed features are ambiguous.
- Feature identities are moderately consistent.

What should I do?
1. Add ticket IDs to commit messages.
2. Use more descriptive branch names.
3. Reference PR numbers consistently.

Expected result:
- Better feature reconstruction.
- Fewer unidentified work streams.
```

### Product Framing

Evolith currently risks presenting itself as a git analytics tool. The stronger framing is:

```text
Project Memory System
```

A user does not primarily care about anchor coverage as a metric. They care whether Evolith can help them understand their project history.

### RFC-006 / TUI Implication

RFC-006 and the future TUI should treat raw reconstruction metrics as supporting evidence, not as the primary user experience.

Human output should lead with:

- Repository health
- Plain-language reasons
- Concrete workflow recommendations
- Expected result

Raw metric labels should remain available in JSON, debug output, or advanced detail views.

---

## Internal Alpha Feedback #1

### Core Shift

Earlier Evolith work optimized for:

```text
Can Evolith reconstruct features correctly?
```

That produced important engineering foundations:

- ARI benchmarks
- Anchor coverage
- Dominant anchor ratio
- Health scores
- Explainability

The next product problem is different:

```text
Can a developer understand the output in 30 seconds?
```

### Biggest Insight

The phrase:

```text
Project Memory System
```

is more valuable than presenting Evolith as git analytics or feature reconstruction. Git analytics and feature reconstruction are implementation details. The user-facing value is:

```text
Help me understand how this project evolved.
```

### Product Assessment

Engine maturity: 8/10

Validated areas:

- Reconstruction
- Anchors
- Persistence
- Explainability
- Cross-repository behavior

User experience: 4/10

This is not because the experience is broken, but because it still exposes internal concepts before user outcomes.

### Design Rule

```text
Meaning > Metrics
```

Do not spend the next iteration primarily tuning clustering algorithms. The larger bottleneck is that the product must make users say:

```text
Oh, now I understand my project.
```

not:

```text
Interesting metrics... what do they mean?
```

### Preferred `evolith doctor` Direction

Start with a summary:

```text
Repository Summary

Your project contains 57 commits grouped into 9 work streams.

Most work streams are identifiable, but 4 remain ambiguous because commit history lacks strong references to tickets, PRs, or feature branches.

Recommended Actions

1. Reference ticket IDs in commit messages.
2. Use descriptive branch names.
3. Link commits to PRs consistently.

Expected Result

Fewer ambiguous work streams and more accurate project history reconstruction.
```

Then put raw metrics below:

```text
Details
-------
Anchor Coverage: 22.8%
Dominant Anchor Ratio: 0.56
```

---

## Alpha Test Quick Start

Thank you for testing Evolith.

Evolith is a Project Memory Doctor. It tells you what your repository's history means, whether it's healthy, and whether anything is at risk.

### Goal

Run Evolith on a repository you know well. The core question is: **can you understand what it says about your project in under 30 seconds?**

### Setup

```bash
cd /path/to/your/project
evolith init
evolith collect git
evolith features rebuild
```

### Start here

```bash
evolith doctor
```

This is the main output. Note your first reaction before reading further.

### Then explore

```bash
evolith doctor --security     # security scan only
evolith features list         # all reconstructed work streams
evolith features explain <id> # deep-dive on one feature
```

### What to look for

1. What was the first line you reacted to?
2. Did you understand the output in under 30 seconds?
3. Does the health diagnosis match your intuition about the repo?
4. Did Evolith catch anything you didn't already know?
5. Would you run this again next week?

### Include with feedback

Please paste:

- Full output of `evolith doctor`
- Output of one accurate feature explanation
- Output of one inaccurate or confusing feature explanation

This helps calibrate both the health scoring and the security detection.

---

## Participant

Name:

Role:

Years of Experience:

---

## Repository

Repository Name:

Approximate Commit Count:

Primary Language:

Project Type:

- Web Application
- Backend Service
- CLI Tool
- Library
- AI/ML Project
- Other

---

## Installation Experience

Time to First Value:

Was setup straightforward?

- Yes
- Mostly
- No

Notes:

---

## Commands Executed

- evolith collect git
- evolith features rebuild
- evolith doctor
- evolith doctor --security
- evolith features list
- evolith features explain

Other:

---

## Most Useful Insight

What was the most useful thing Evolith showed you?

---

## Reaction Line

Which single output line made you react most — positively or negatively?

Examples of what this might look like:

```
🚨 Database credentials found in BACKEND_GUIDE.md
21 work streams identified
7 ambiguous work streams
Only 23% of commits contain identity signals
No security issues detected
```

Your answer:

---

## Most Confusing Output

What output was difficult to understand?

---

## Accuracy Assessment

Which reconstructed features felt accurate?

---

Which reconstructed features felt incorrect?

---

## Health & Explainability

Did Anchor Confidence help you trust the output?

- Yes
- Somewhat
- No

Did Evidence Strength help you understand reliability?

- Yes
- Somewhat
- No

Did Feature Health feel useful?

- Yes
- Somewhat
- No

Comments:

---

## Recommendations

What action did Evolith suggest that you would actually take?

---

What recommendations were not useful?

---

## Missing Features

What did you expect Evolith to do that it currently does not?

---

## Overall

Would you use Evolith again?

- Definitely
- Maybe
- No

Would you recommend it to another developer?

- Yes
- Maybe
- No

Additional Feedback:
