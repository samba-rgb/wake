#!/usr/bin/env python3

"""
Performance Visualization Script for Wake vs Stern Benchmark
Generates charts and graphs from benchmark CSV data
"""

import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import sys
import os
from datetime import datetime
import argparse

# Set style for better looking plots
plt.style.use('seaborn-v0_8')
sns.set_palette("husl")

def load_data(csv_file):
    """Load benchmark data from CSV file."""
    try:
        df = pd.read_csv(csv_file)
        df['timestamp'] = pd.to_datetime(df['timestamp'])
        return df
    except Exception as e:
        print(f"Error loading data: {e}")
        sys.exit(1)

def create_cpu_comparison(df, output_dir):
    """Create CPU usage comparison charts."""
    fig, axes = plt.subplots(2, 2, figsize=(15, 12))
    fig.suptitle('CPU Performance Comparison: Wake vs Stern', fontsize=16, fontweight='bold')
    
    scenarios = df['scenario'].unique()
    
    # Plot 1: CPU usage over time for each scenario
    ax1 = axes[0, 0]
    for scenario in scenarios:
        scenario_data = df[df['scenario'] == scenario]
        for tool in ['wake', 'stern']:
            tool_data = scenario_data[scenario_data['tool'] == tool]
            if not tool_data.empty:
                ax1.plot(tool_data['duration_seconds'], tool_data['cpu_percent'], 
                        label=f'{tool.title()} - {scenario}', marker='o', markersize=2)
    
    ax1.set_xlabel('Duration (seconds)')
    ax1.set_ylabel('CPU Usage (%)')
    ax1.set_title('CPU Usage Over Time')
    ax1.legend(bbox_to_anchor=(1.05, 1), loc='upper left')
    ax1.grid(True, alpha=0.3)
    
    # Plot 2: Average CPU by scenario
    ax2 = axes[0, 1]
    cpu_avg = df.groupby(['scenario', 'tool'])['cpu_percent'].mean().unstack()
    cpu_avg.plot(kind='bar', ax=ax2, width=0.8)
    ax2.set_xlabel('Scenario')
    ax2.set_ylabel('Average CPU Usage (%)')
    ax2.set_title('Average CPU Usage by Scenario')
    ax2.legend(title='Tool')
    ax2.tick_params(axis='x', rotation=45)
    
    # Plot 3: Max CPU by scenario
    ax3 = axes[1, 0]
    cpu_max = df.groupby(['scenario', 'tool'])['cpu_percent'].max().unstack()
    cpu_max.plot(kind='bar', ax=ax3, width=0.8)
    ax3.set_xlabel('Scenario')
    ax3.set_ylabel('Maximum CPU Usage (%)')
    ax3.set_title('Maximum CPU Usage by Scenario')
    ax3.legend(title='Tool')
    ax3.tick_params(axis='x', rotation=45)
    
    # Plot 4: CPU distribution boxplot
    ax4 = axes[1, 1]
    sns.boxplot(data=df, x='scenario', y='cpu_percent', hue='tool', ax=ax4)
    ax4.set_xlabel('Scenario')
    ax4.set_ylabel('CPU Usage (%)')
    ax4.set_title('CPU Usage Distribution')
    ax4.tick_params(axis='x', rotation=45)
    
    plt.tight_layout()
    plt.savefig(f'{output_dir}/cpu_comparison.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    print(f"CPU comparison chart saved to: {output_dir}/cpu_comparison.png")

def create_memory_comparison(df, output_dir):
    """Create memory usage comparison charts."""
    fig, axes = plt.subplots(2, 2, figsize=(15, 12))
    fig.suptitle('Memory Performance Comparison: Wake vs Stern', fontsize=16, fontweight='bold')
    
    scenarios = df['scenario'].unique()
    
    # Plot 1: Memory usage over time
    ax1 = axes[0, 0]
    for scenario in scenarios:
        scenario_data = df[df['scenario'] == scenario]
        for tool in ['wake', 'stern']:
            tool_data = scenario_data[scenario_data['tool'] == tool]
            if not tool_data.empty:
                ax1.plot(tool_data['duration_seconds'], tool_data['memory_mb'], 
                        label=f'{tool.title()} - {scenario}', marker='o', markersize=2)
    
    ax1.set_xlabel('Duration (seconds)')
    ax1.set_ylabel('Memory Usage (MB)')
    ax1.set_title('Memory Usage Over Time')
    ax1.legend(bbox_to_anchor=(1.05, 1), loc='upper left')
    ax1.grid(True, alpha=0.3)
    
    # Plot 2: Average memory by scenario
    ax2 = axes[0, 1]
    mem_avg = df.groupby(['scenario', 'tool'])['memory_mb'].mean().unstack()
    mem_avg.plot(kind='bar', ax=ax2, width=0.8)
    ax2.set_xlabel('Scenario')
    ax2.set_ylabel('Average Memory Usage (MB)')
    ax2.set_title('Average Memory Usage by Scenario')
    ax2.legend(title='Tool')
    ax2.tick_params(axis='x', rotation=45)
    
    # Plot 3: Max memory by scenario
    ax3 = axes[1, 0]
    mem_max = df.groupby(['scenario', 'tool'])['memory_mb'].max().unstack()
    mem_max.plot(kind='bar', ax=ax3, width=0.8)
    ax3.set_xlabel('Scenario')
    ax3.set_ylabel('Maximum Memory Usage (MB)')
    ax3.set_title('Maximum Memory Usage by Scenario')
    ax3.legend(title='Tool')
    ax3.tick_params(axis='x', rotation=45)
    
    # Plot 4: Memory distribution boxplot
    ax4 = axes[1, 1]
    sns.boxplot(data=df, x='scenario', y='memory_mb', hue='tool', ax=ax4)
    ax4.set_xlabel('Scenario')
    ax4.set_ylabel('Memory Usage (MB)')
    ax4.set_title('Memory Usage Distribution')
    ax4.tick_params(axis='x', rotation=45)
    
    plt.tight_layout()
    plt.savefig(f'{output_dir}/memory_comparison.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    print(f"Memory comparison chart saved to: {output_dir}/memory_comparison.png")

def create_performance_summary(df, output_dir):
    """Create overall performance summary chart."""
    fig, axes = plt.subplots(1, 2, figsize=(14, 6))
    fig.suptitle('Performance Summary: Wake vs Stern', fontsize=16, fontweight='bold')
    
    # Calculate normalized performance scores (lower is better)
    summary_data = []
    for scenario in df['scenario'].unique():
        for tool in ['wake', 'stern']:
            tool_data = df[(df['scenario'] == scenario) & (df['tool'] == tool)]
            if not tool_data.empty:
                avg_cpu = tool_data['cpu_percent'].mean()
                avg_memory = tool_data['memory_mb'].mean()
                max_cpu = tool_data['cpu_percent'].max()
                max_memory = tool_data['memory_mb'].max()
                
                summary_data.append({
                    'scenario': scenario,
                    'tool': tool,
                    'avg_cpu': avg_cpu,
                    'avg_memory': avg_memory,
                    'max_cpu': max_cpu,
                    'max_memory': max_memory,
                    'performance_score': (avg_cpu + avg_memory/10)  # Weighted score
                })
    
    summary_df = pd.DataFrame(summary_data)
    
    # Plot 1: Performance score by scenario
    ax1 = axes[0]
    perf_pivot = summary_df.pivot(index='scenario', columns='tool', values='performance_score')
    perf_pivot.plot(kind='bar', ax=ax1, width=0.8, color=['#1f77b4', '#ff7f0e'])
    ax1.set_xlabel('Scenario')
    ax1.set_ylabel('Performance Score (lower is better)')
    ax1.set_title('Overall Performance Score')
    ax1.legend(title='Tool')
    ax1.tick_params(axis='x', rotation=45)
    
    # Plot 2: Resource efficiency scatter
    ax2 = axes[1]
    for tool in ['wake', 'stern']:
        tool_summary = summary_df[summary_df['tool'] == tool]
        ax2.scatter(tool_summary['avg_cpu'], tool_summary['avg_memory'], 
                   label=tool.title(), s=100, alpha=0.7)
        
        # Add scenario labels
        for _, row in tool_summary.iterrows():
            ax2.annotate(row['scenario'], (row['avg_cpu'], row['avg_memory']), 
                        xytext=(5, 5), textcoords='offset points', fontsize=8)
    
    ax2.set_xlabel('Average CPU Usage (%)')
    ax2.set_ylabel('Average Memory Usage (MB)')
    ax2.set_title('Resource Efficiency (closer to origin is better)')
    ax2.legend()
    ax2.grid(True, alpha=0.3)
    
    plt.tight_layout()
    plt.savefig(f'{output_dir}/performance_summary.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    print(f"Performance summary chart saved to: {output_dir}/performance_summary.png")

def create_detailed_report(df, output_dir):
    """Create a detailed performance analysis report."""
    report_file = f'{output_dir}/detailed_analysis.md'
    
    with open(report_file, 'w') as f:
        f.write("# Detailed Performance Analysis\n\n")
        f.write(f"**Generated:** {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n")
        
        # Overall statistics
        f.write("## Overall Statistics\n\n")
        f.write("| Tool | Avg CPU (%) | Max CPU (%) | Avg Memory (MB) | Max Memory (MB) |\n")
        f.write("|------|-------------|-------------|-----------------|------------------|\n")
        
        for tool in ['wake', 'stern']:
            tool_data = df[df['tool'] == tool]
            if not tool_data.empty:
                avg_cpu = tool_data['cpu_percent'].mean()
                max_cpu = tool_data['cpu_percent'].max()
                avg_mem = tool_data['memory_mb'].mean()
                max_mem = tool_data['memory_mb'].max()
                
                f.write(f"| {tool.title()} | {avg_cpu:.2f} | {max_cpu:.2f} | {avg_mem:.2f} | {max_mem:.2f} |\n")
        
        f.write("\n## Scenario Breakdown\n\n")
        
        for scenario in df['scenario'].unique():
            f.write(f"### {scenario.title()} Scenario\n\n")
            scenario_data = df[df['scenario'] == scenario]
            
            f.write("| Tool | Avg CPU (%) | Max CPU (%) | Avg Memory (MB) | Max Memory (MB) | Data Points |\n")
            f.write("|------|-------------|-------------|-----------------|-----------------|-------------|\n")
            
            for tool in ['wake', 'stern']:
                tool_data = scenario_data[scenario_data['tool'] == tool]
                if not tool_data.empty:
                    avg_cpu = tool_data['cpu_percent'].mean()
                    max_cpu = tool_data['cpu_percent'].max()
                    avg_mem = tool_data['memory_mb'].mean()
                    max_mem = tool_data['memory_mb'].max()
                    count = len(tool_data)
                    
                    f.write(f"| {tool.title()} | {avg_cpu:.2f} | {max_cpu:.2f} | {avg_mem:.2f} | {max_mem:.2f} | {count} |\n")
            
            f.write("\n")
        
        # Performance insights
        f.write("## Performance Insights\n\n")
        
        # CPU efficiency analysis
        wake_avg_cpu = df[df['tool'] == 'wake']['cpu_percent'].mean()
        stern_avg_cpu = df[df['tool'] == 'stern']['cpu_percent'].mean()
        
        if wake_avg_cpu < stern_avg_cpu:
            cpu_winner = "Wake"
            cpu_improvement = ((stern_avg_cpu - wake_avg_cpu) / stern_avg_cpu) * 100
        else:
            cpu_winner = "Stern"
            cpu_improvement = ((wake_avg_cpu - stern_avg_cpu) / wake_avg_cpu) * 100
        
        f.write(f"- **CPU Efficiency Winner:** {cpu_winner} (by {cpu_improvement:.1f}%)\n")
        
        # Memory efficiency analysis
        wake_avg_mem = df[df['tool'] == 'wake']['memory_mb'].mean()
        stern_avg_mem = df[df['tool'] == 'stern']['memory_mb'].mean()
        
        if wake_avg_mem < stern_avg_mem:
            mem_winner = "Wake"
            mem_improvement = ((stern_avg_mem - wake_avg_mem) / stern_avg_mem) * 100
        else:
            mem_winner = "Stern"
            mem_improvement = ((wake_avg_mem - stern_avg_mem) / wake_avg_mem) * 100
        
        f.write(f"- **Memory Efficiency Winner:** {mem_winner} (by {mem_improvement:.1f}%)\n")
        
        f.write(f"\n## Data Quality\n\n")
        f.write(f"- **Total data points:** {len(df)}\n")
        f.write(f"- **Wake data points:** {len(df[df['tool'] == 'wake'])}\n")
        f.write(f"- **Stern data points:** {len(df[df['tool'] == 'stern'])}\n")
        f.write(f"- **Scenarios tested:** {len(df['scenario'].unique())}\n")
    
    print(f"Detailed analysis report saved to: {report_file}")

def main():
    parser = argparse.ArgumentParser(description='Generate performance visualization charts')
    parser.add_argument('csv_file', help='Path to the benchmark CSV file')
    parser.add_argument('--output-dir', '-o', default=None, 
                       help='Output directory for charts (default: same as CSV file)')
    
    args = parser.parse_args()
    
    if not os.path.exists(args.csv_file):
        print(f"Error: CSV file not found: {args.csv_file}")
        sys.exit(1)
    
    # Determine output directory
    if args.output_dir:
        output_dir = args.output_dir
    else:
        output_dir = os.path.dirname(args.csv_file)
        if not output_dir:
            output_dir = '.'
    
    # Create output directory if it doesn't exist
    os.makedirs(output_dir, exist_ok=True)
    
    print(f"Loading benchmark data from: {args.csv_file}")
    df = load_data(args.csv_file)
    
    print(f"Loaded {len(df)} data points")
    print(f"Tools: {df['tool'].unique()}")
    print(f"Scenarios: {df['scenario'].unique()}")
    
    # Generate visualizations
    print("\nGenerating visualizations...")
    create_cpu_comparison(df, output_dir)
    create_memory_comparison(df, output_dir)
    create_performance_summary(df, output_dir)
    create_detailed_report(df, output_dir)
    
    print(f"\nAll visualizations saved to: {output_dir}")
    print("Generated files:")
    print("- cpu_comparison.png")
    print("- memory_comparison.png") 
    print("- performance_summary.png")
    print("- detailed_analysis.md")

if __name__ == "__main__":
    main()