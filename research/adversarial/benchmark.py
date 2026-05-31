import sys
import os
import random
import numpy as np
from sklearn.metrics import adjusted_rand_score, normalized_mutual_info_score
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from adversarial.scenarios import (
    scenario_a_shared_infrastructure,
    scenario_b_extreme_context_switching,
    scenario_c_missing_metadata,
    scenario_d_long_lived_feature,
    scenario_e_feature_twins,
    scenario_f_monorepo,
    scenario_g_giant_utility_file,
    scenario_e2_narrative_twins,
    scenario_g2_narrative_hub,
    scenario_e3_interrupted_narrative,
    scenario_e2a_twin_with_ticket_anchors,
    scenario_e2b_twin_with_branch_episodes,
    scenario_e2c_twin_with_pr_anchors,
)
from feature_reconstruction_simulator import evaluate_clusters
from notebooks.clustering_experiments import build_louvain_clusters

def run_benchmark(scenario_func, name):
    print(f"\n========================================")
    print(f" BENCHMARK: {name}")
    print(f"========================================")
    events = scenario_func(1000)
    
    # We use Louvain community detection
    clusters = build_louvain_clusters(events, threshold=0.15)
    
    correct = 0
    total_events = len(events)
    for i, cluster in enumerate(clusters):
        truth_counts = {}
        for e in cluster:
            truth_counts[e['ground_truth']] = truth_counts.get(e['ground_truth'], 0) + 1
            
        dominant_truth = max(truth_counts.items(), key=lambda x: x[1])[0]
        correct += truth_counts[dominant_truth]
        
    accuracy = correct / total_events
    print(f"--> LOUVAIN PURITY: {accuracy*100:.2f}%\n")

    # Stricter quality checks to catch purity inflation from tiny clusters.
    gt_labels = [e["ground_truth"] for e in events]
    event_index = {e["event_id"]: i for i, e in enumerate(events)}
    pred_labels = [-1] * total_events
    for cluster_id, cluster in enumerate(clusters):
        for event in cluster:
            pred_labels[event_index[event["event_id"]]] = cluster_id

    singleton_count = sum(1 for c in clusters if len(c) == 1)
    singleton_ratio = singleton_count / len(clusters) if clusters else 0.0
    ari = adjusted_rand_score(gt_labels, pred_labels)
    nmi = normalized_mutual_info_score(gt_labels, pred_labels)

    print(f"--> CLUSTERS: {len(clusters)}")
    print(f"--> SINGLETON CLUSTERS: {singleton_count} ({singleton_ratio*100:.2f}%)")
    print(f"--> ARI: {ari:.4f}")
    print(f"--> NMI: {nmi:.4f}\n")
    return {"purity": accuracy, "ari": ari, "nmi": nmi}

if __name__ == '__main__':
    random.seed(42)
    np.random.seed(42)
    results = {}
    results["A"] = run_benchmark(scenario_a_shared_infrastructure, "Scenario A: Shared Infrastructure")
    results["B"] = run_benchmark(scenario_b_extreme_context_switching, "Scenario B: Extreme Context Switching")
    results["C"] = run_benchmark(scenario_c_missing_metadata, "Scenario C: Missing Metadata")
    results["D"] = run_benchmark(scenario_d_long_lived_feature, "Scenario D: Long-Lived Feature")
    results["E"] = run_benchmark(scenario_e_feature_twins, "Scenario E: Feature Twins")
    results["F"] = run_benchmark(scenario_f_monorepo, "Scenario F: Monorepo")
    results["G"] = run_benchmark(scenario_g_giant_utility_file, "Scenario G: Giant Utility File")
    results["E2"] = run_benchmark(scenario_e2_narrative_twins, "Scenario E2: Narrative Twins")
    results["G2"] = run_benchmark(scenario_g2_narrative_hub, "Scenario G2: Narrative Hub")
    results["E3"] = run_benchmark(scenario_e3_interrupted_narrative, "Scenario E3: Interrupted Narrative")
    results["E2A"] = run_benchmark(scenario_e2a_twin_with_ticket_anchors, "Scenario E2A: Twins + Ticket Anchors")
    results["E2B"] = run_benchmark(scenario_e2b_twin_with_branch_episodes, "Scenario E2B: Twins + Branch Episodes")
    results["E2C"] = run_benchmark(scenario_e2c_twin_with_pr_anchors, "Scenario E2C: Twins + PR Anchors")

    print("========================================")
    print(" RESEARCH-002 ACCEPTANCE GATES (ARI)")
    print("========================================")
    normal_ari = (results["A"]["ari"] + results["B"]["ari"] + results["C"]["ari"] + results["D"]["ari"]) / 4.0
    print(f"Normal Dataset ARI>0.90: {normal_ari:.4f} -> {'PASS' if normal_ari > 0.90 else 'FAIL'}")
    print(f"Feature Twins ARI>0.70: {results['E']['ari']:.4f} -> {'PASS' if results['E']['ari'] > 0.70 else 'FAIL'}")
    print(f"Utility Hub ARI>0.70: {results['G']['ari']:.4f} -> {'PASS' if results['G']['ari'] > 0.70 else 'FAIL'}")
    print(f"Monorepo ARI>0.85: {results['F']['ari']:.4f} -> {'PASS' if results['F']['ari'] > 0.85 else 'FAIL'}")
    print("")
    print("========================================")
    print(" RESEARCH-004 NARRATIVE GATES (ARI)")
    print("========================================")
    print(f"Narrative Twins E2 ARI>0.40: {results['E2']['ari']:.4f} -> {'PASS' if results['E2']['ari'] > 0.40 else 'FAIL'}")
    print(f"Narrative Hub G2 ARI>0.40: {results['G2']['ari']:.4f} -> {'PASS' if results['G2']['ari'] > 0.40 else 'FAIL'}")
    print(f"Interrupted Narrative E3 ARI>0.40: {results['E3']['ari']:.4f} -> {'PASS' if results['E3']['ari'] > 0.40 else 'FAIL'}")
    print("")
    print("========================================")
    print(" RESEARCH-005 ANCHOR GATES (ARI)")
    print("========================================")
    print(f"E2A Tickets ARI>0.80: {results['E2A']['ari']:.4f} -> {'PASS' if results['E2A']['ari'] > 0.80 else 'FAIL'}")
    print(f"E2B Branch Episodes ARI>0.80: {results['E2B']['ari']:.4f} -> {'PASS' if results['E2B']['ari'] > 0.80 else 'FAIL'}")
    print(f"E2C PR Anchors ARI>0.80: {results['E2C']['ari']:.4f} -> {'PASS' if results['E2C']['ari'] > 0.80 else 'FAIL'}")
