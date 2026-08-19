"""
Label-leakage guard for the adversarial benchmark.

An evaluation is only meaningful if the inputs a method reads are not a
relabeling of the ground truth it is scored against. RESEARCH-005's anchor
scenarios failed this: `ticket_id` was assigned from the same dict literal as
`ground_truth`, one value per feature, so grouping by the anchor reproduced the
label exactly and ARI 1.000 was arithmetically forced.

This module measures that relationship directly so a scenario cannot claim a
score without also declaring how much of the answer it handed the method.

Verdicts:
    LEAK       the field determines the label (a groupby on it scores ~1.0)
    STRONG     the field carries most of the label, scores will be inflated
    INFORMATIVE the field carries real but partial signal (what we want)
    WEAK       the field carries almost nothing
"""

from collections import defaultdict

from sklearn.metrics import (
    adjusted_rand_score,
    normalized_mutual_info_score,
)

# Fields a similarity function is allowed to read. Anything here is fair game
# for a method, and therefore must be audited against the label.
INPUT_FIELDS = ("ticket_id", "pr_id", "branch", "file_path", "commit_msg")

LEAK_NMI = 0.99
STRONG_NMI = 0.70
INFORMATIVE_NMI = 0.15


def _verdict(nmi, groupby_ari):
    if nmi >= LEAK_NMI or groupby_ari >= LEAK_NMI:
        return "LEAK"
    if nmi >= STRONG_NMI:
        return "STRONG"
    if nmi >= INFORMATIVE_NMI:
        return "INFORMATIVE"
    return "WEAK"


def field_report(events, field):
    """Measure how much of `ground_truth` the given input field encodes."""
    labels = [e["ground_truth"] for e in events]
    values = [e.get(field, "") or "<empty>" for e in events]

    coverage = sum(1 for v in values if v != "<empty>") / max(1, len(values))
    nmi = normalized_mutual_info_score(labels, values)

    # The decisive test: cluster by simply grouping on this field. If that ties
    # the method under evaluation, the method is not contributing anything.
    groupby_ari = adjusted_rand_score(labels, values)

    mapping = defaultdict(set)
    for label, value in zip(labels, values):
        mapping[label].add(value)
    distinct_values = len({v for v in values})
    one_value_per_label = all(len(v) == 1 for v in mapping.values())
    bijective = one_value_per_label and distinct_values == len(mapping)

    return {
        "field": field,
        "coverage": coverage,
        "nmi": nmi,
        "groupby_ari": groupby_ari,
        "distinct_values": distinct_values,
        "bijective": bijective,
        "verdict": _verdict(nmi, groupby_ari),
    }


def audit_scenario(events, fields=INPUT_FIELDS):
    """Audit every input field a similarity function may read."""
    return [field_report(events, f) for f in fields]


def leaking_fields(events, fields=INPUT_FIELDS):
    return [r["field"] for r in audit_scenario(events, fields) if r["verdict"] == "LEAK"]


def format_report(reports, indent="    "):
    lines = [
        f"{indent}{'field':<12} {'cover':>6} {'NMI':>7} {'groupby':>8} "
        f"{'vals':>5} {'1:1':>4}  verdict",
        f"{indent}{'-' * 58}",
    ]
    for r in reports:
        lines.append(
            f"{indent}{r['field']:<12} {r['coverage']*100:5.1f}% {r['nmi']:7.4f} "
            f"{r['groupby_ari']:8.4f} {r['distinct_values']:5d} "
            f"{'yes' if r['bijective'] else 'no':>4}  {r['verdict']}"
        )
    leaks = [r for r in reports if r["verdict"] == "LEAK"]
    if leaks:
        names = ", ".join(r["field"] for r in leaks)
        lines.append("")
        lines.append(
            f"{indent}!! LEAK: {names} determines ground_truth. Any score on this"
        )
        lines.append(
            f"{indent}   scenario measures an upper bound, not method performance."
        )
    return "\n".join(lines)


if __name__ == "__main__":
    import os
    import random
    import sys

    sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

    from adversarial.scenarios import (
        scenario_e2_narrative_twins,
        scenario_e2a_twin_with_ticket_anchors,
        scenario_e2d_realistic_ticket_anchors,
    )

    for name, fn in [
        ("E2  narrative twins (no anchor)", scenario_e2_narrative_twins),
        ("E2A twins + ticket anchors", scenario_e2a_twin_with_ticket_anchors),
        ("E2D twins + realistic tickets", scenario_e2d_realistic_ticket_anchors),
    ]:
        random.seed(42)
        print(f"\n{name}")
        print(format_report(audit_scenario(fn(600))))
