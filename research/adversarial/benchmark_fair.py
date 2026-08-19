"""
Like-for-like benchmark for feature reconstruction.

The original `benchmark.py` calls `build_louvain_clusters` and nothing else, so
its "baselines" are never actually run, and it reports a single seed. That makes
its headline comparison (ARI 0.018 vs 1.000) a comparison of one method against
itself on two different datasets, not a comparison of methods.

This harness fixes three things:

1. Every method sees the same events and the same similarity function, so the
   comparison is method-vs-method on one dataset.
2. A trivial `groupby(field)` baseline is always included. If a one-line groupby
   ties the graph method, the graph method is not contributing anything, and we
   want that visible rather than buried.
3. Results are averaged over multiple seeds with standard deviation, and every
   scenario is run through the leakage guard first.

Similarities are computed once per (scenario, seed) and shared across methods,
which is what keeps the O(N^2) cost affordable.

Usage:
    python adversarial/benchmark_fair.py
    python adversarial/benchmark_fair.py --events 1000 --seeds 10
"""

import argparse
import itertools
import os
import random
import sys
from collections import defaultdict

import numpy as np
from sklearn.cluster import DBSCAN
from sklearn.metrics import adjusted_rand_score, normalized_mutual_info_score

sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

import networkx as nx
from networkx.algorithms.community import louvain_communities

from adversarial.leakage_guard import INPUT_FIELDS, audit_scenario, format_report
from adversarial.scenarios import (
    scenario_a_shared_infrastructure,
    scenario_b_extreme_context_switching,
    scenario_c_missing_metadata,
    scenario_d_long_lived_feature,
    scenario_e_feature_twins,
    scenario_e2_narrative_twins,
    scenario_e2a_twin_with_ticket_anchors,
    scenario_e2b_twin_with_branch_episodes,
    scenario_e2c_twin_with_pr_anchors,
    scenario_e2d_realistic_ticket_anchors,
    scenario_e3_interrupted_narrative,
    scenario_f_monorepo,
    scenario_g_giant_utility_file,
    scenario_g2_narrative_hub,
)
from feature_reconstruction_simulator import (
    assign_episode_ids,
    calculate_similarity,
    calculate_similarity_hybrid,
    compute_file_idf,
)


# --------------------------------------------------------------------------
# Shared similarity computation
# --------------------------------------------------------------------------

def similarity_matrix(events, mode="hybrid"):
    """Compute the full pairwise similarity once so every method can share it."""
    n = len(events)
    matrix = np.zeros((n, n), dtype=float)
    idf_map, min_idf, max_idf = compute_file_idf(events)
    episode_map = assign_episode_ids(events) if mode == "hybrid" else None

    for i, j in itertools.combinations(range(n), 2):
        if mode == "hybrid":
            sim = calculate_similarity_hybrid(
                events[i], events[j], idf_map, min_idf, max_idf, episode_map=episode_map
            )
        else:
            sim = calculate_similarity(events[i], events[j])
        matrix[i, j] = matrix[j, i] = sim
    return matrix


# --------------------------------------------------------------------------
# Methods. Each returns a label per event, in event order.
# --------------------------------------------------------------------------

def louvain_labels(events, matrix, threshold=0.15, hub_penalty=True, seed=0):
    n = len(events)
    graph = nx.Graph()
    graph.add_nodes_from(range(n))
    rows, cols = np.where(np.triu(matrix, k=1) > threshold)
    for i, j in zip(rows, cols):
        graph.add_edge(int(i), int(j), weight=float(matrix[i, j]))

    if hub_penalty and graph.number_of_edges() > 0:
        for u, v, data in graph.edges(data=True):
            data["weight"] /= np.sqrt(max(1, graph.degree(u)) * max(1, graph.degree(v)))

    labels = [-1] * n
    for cid, community in enumerate(louvain_communities(graph, weight="weight", seed=seed)):
        for node in community:
            labels[node] = cid
    return labels


def dbscan_labels(events, matrix, eps=0.5, min_samples=3):
    distance = np.clip(1.0 - matrix, 0.0, 1.0)
    np.fill_diagonal(distance, 0.0)
    return list(DBSCAN(eps=eps, min_samples=min_samples, metric="precomputed").fit_predict(distance))


def connected_components_labels(events, matrix, threshold=0.39):
    n = len(events)
    parent = list(range(n))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    rows, cols = np.where(np.triu(matrix, k=1) >= threshold)
    for i, j in zip(rows, cols):
        ri, rj = find(int(i)), find(int(j))
        if ri != rj:
            parent[ri] = rj
    return [find(i) for i in range(n)]


def groupby_labels(events, field):
    return [e.get(field, "") or "<empty>" for e in events]


def best_groupby(events, fields=INPUT_FIELDS):
    """The strongest trivial baseline: group by whichever single field wins."""
    truth = [e["ground_truth"] for e in events]
    best_field, best_score = None, -1.0
    for field in fields:
        score = adjusted_rand_score(truth, groupby_labels(events, field))
        if score > best_score:
            best_field, best_score = field, score
    return best_field, best_score


# --------------------------------------------------------------------------
# Runner
# --------------------------------------------------------------------------

SCENARIOS = [
    ("A   shared infrastructure", scenario_a_shared_infrastructure),
    ("B   context switching", scenario_b_extreme_context_switching),
    ("C   missing metadata", scenario_c_missing_metadata),
    ("D   long-lived feature", scenario_d_long_lived_feature),
    ("E   feature twins", scenario_e_feature_twins),
    ("F   monorepo", scenario_f_monorepo),
    ("G   giant utility file", scenario_g_giant_utility_file),
    ("E2  narrative twins", scenario_e2_narrative_twins),
    ("G2  narrative hub", scenario_g2_narrative_hub),
    ("E3  interrupted narrative", scenario_e3_interrupted_narrative),
    ("E2A twins + ticket anchors", scenario_e2a_twin_with_ticket_anchors),
    ("E2B twins + branch episodes", scenario_e2b_twin_with_branch_episodes),
    ("E2C twins + PR anchors", scenario_e2c_twin_with_pr_anchors),
    ("E2D twins + realistic tickets", scenario_e2d_realistic_ticket_anchors),
]


def run_scenario(scenario_fn, num_events, seeds):
    per_method = defaultdict(list)
    leak_reports = None
    trivial_fields = []

    for seed in seeds:
        random.seed(seed)
        np.random.seed(seed)
        events = scenario_fn(num_events)
        truth = [e["ground_truth"] for e in events]

        if leak_reports is None:
            leak_reports = audit_scenario(events)

        hybrid = similarity_matrix(events, "hybrid")
        plain = similarity_matrix(events, "plain")

        field, score = best_groupby(events)
        trivial_fields.append(field)

        runs = {
            "louvain (anchor-aware)": louvain_labels(events, hybrid, seed=seed),
            "louvain (anchor-blind)": louvain_labels(events, plain, seed=seed),
            "dbscan (anchor-aware)": dbscan_labels(events, hybrid),
            "conn-components": connected_components_labels(events, hybrid),
            f"groupby (trivial)": groupby_labels(events, field),
        }
        for name, labels in runs.items():
            per_method[name].append(
                (
                    adjusted_rand_score(truth, labels),
                    normalized_mutual_info_score(truth, labels),
                    len(set(labels) - {-1}),
                )
            )

    return per_method, leak_reports, trivial_fields


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--events", type=int, default=600)
    parser.add_argument("--seeds", type=int, default=5)
    parser.add_argument("--only", type=str, default=None, help="substring filter on scenario name")
    args = parser.parse_args()

    seeds = list(range(args.seeds))
    scenarios = [s for s in SCENARIOS if not args.only or args.only.lower() in s[0].lower()]

    print("=" * 78)
    print(f" LIKE-FOR-LIKE BENCHMARK  |  {args.events} events  |  seeds {seeds}")
    print("=" * 78)
    print(" Every method sees the same events and the same similarity function.")
    print(" 'groupby (trivial)' groups by a single raw field. A graph method that")
    print(" does not beat it is not contributing anything.")

    summary = {}
    for name, fn in scenarios:
        per_method, leaks, fields = run_scenario(fn, args.events, seeds)

        print(f"\n{'-' * 78}\n {name}\n{'-' * 78}")
        print(format_report(leaks))
        print()
        print(f"    {'method':<24} {'ARI mean':>9} {'sd':>7} {'NMI':>7} {'clusters':>9}")
        print(f"    {'-' * 60}")
        for method, runs in per_method.items():
            aris = [r[0] for r in runs]
            nmis = [r[1] for r in runs]
            counts = [r[2] for r in runs]
            label = method
            if method.startswith("groupby"):
                label = f"groupby({fields[0]})"
            print(
                f"    {label:<24} {np.mean(aris):9.4f} {np.std(aris):7.4f} "
                f"{np.mean(nmis):7.4f} {np.mean(counts):9.1f}"
            )
        summary[name] = per_method

    print(f"\n{'=' * 78}\n HONESTY CHECKS\n{'=' * 78}")
    for name, per_method in summary.items():
        best_graph = np.mean([r[0] for r in per_method["louvain (anchor-aware)"]])
        trivial = np.mean([r[0] for r in per_method["groupby (trivial)"]])
        margin = best_graph - trivial
        flag = "OK  " if margin > 0.05 else "FLAG"
        print(
            f" {flag} {name:<32} louvain {best_graph:7.4f} vs trivial {trivial:7.4f}"
            f"  (margin {margin:+.4f})"
        )
    print()
    print(" FLAG = the graph method does not meaningfully beat a one-line groupby")
    print(" on this scenario, so the scenario does not support a claim about the method.")


if __name__ == "__main__":
    main()
