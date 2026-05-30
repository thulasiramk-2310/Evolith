import sys
import os
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from adversarial.scenarios import (
    scenario_a_shared_infrastructure,
    scenario_b_extreme_context_switching,
    scenario_c_missing_metadata,
    scenario_d_long_lived_feature
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

if __name__ == '__main__':
    run_benchmark(scenario_a_shared_infrastructure, "Scenario A: Shared Infrastructure")
    run_benchmark(scenario_b_extreme_context_switching, "Scenario B: Extreme Context Switching")
    run_benchmark(scenario_c_missing_metadata, "Scenario C: Missing Metadata")
    run_benchmark(scenario_d_long_lived_feature, "Scenario D: Long-Lived Feature")
