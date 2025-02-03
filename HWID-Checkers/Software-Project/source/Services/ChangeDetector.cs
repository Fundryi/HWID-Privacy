using System.Collections.Generic;
using System.Linq;
using HWIDChecker.Services.Interfaces;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services
{
    public class ChangeDetector : IChangeDetector
    {
        private readonly Dictionary<string, IComponentIdentifierStrategy> _strategies;

        public ChangeDetector(Dictionary<string, IComponentIdentifierStrategy> strategies)
        {
            _strategies = strategies;
        }

        public List<ComparisonResult> DetectChanges(
            List<(ComponentIdentifier Base, ComponentIdentifier Target)> matches)
        {
            var results = new List<ComparisonResult>();

            foreach (var match in matches)
            {
                if (match.Base == null && match.Target != null)
                {
                    // Component was added
                    var result = new ComparisonResult(match.Target.Type, ChangeType.Added);
                    AddAllPropertiesAsChanges(result, null, match.Target);
                    results.Add(result);
                }
                else if (match.Base != null && match.Target == null)
                {
                    // Component was removed
                    var result = new ComparisonResult(match.Base.Type, ChangeType.Removed);
                    AddAllPropertiesAsChanges(result, match.Base, null);
                    results.Add(result);
                }
                else if (match.Base != null && match.Target != null)
                {
                    // Compare properties for modifications
                    var result = CompareProperties(match.Base, match.Target);
                    if (result != null)
                    {
                        results.Add(result);
                    }
                }
            }

            return results;
        }

        private ComparisonResult CompareProperties(ComponentIdentifier baseComponent, ComponentIdentifier targetComponent)
        {
            if (!_strategies.TryGetValue(baseComponent.Type, out var strategy))
                return null;

            var comparisonProperties = strategy.GetComparisonProperties();
            var changes = new Dictionary<string, (string OldValue, string NewValue)>();

            foreach (var property in comparisonProperties)
            {
                var baseValue = baseComponent.GetProperty(property);
                var targetValue = targetComponent.GetProperty(property);

                if (baseValue != targetValue)
                {
                    changes[property] = (baseValue, targetValue);
                }
            }

            if (changes.Any())
            {
                var result = new ComparisonResult(baseComponent.Type, ChangeType.Modified);
                foreach (var change in changes)
                {
                    result.AddChange(change.Key, change.Value.OldValue, change.Value.NewValue);
                }
                return result;
            }

            return null;
        }

        private void AddAllPropertiesAsChanges(
            ComparisonResult result,
            ComponentIdentifier baseComponent,
            ComponentIdentifier targetComponent)
        {
            if (!_strategies.TryGetValue(result.ComponentType, out var strategy))
                return;

            var properties = strategy.GetComparisonProperties();

            foreach (var property in properties)
            {
                var oldValue = baseComponent?.GetProperty(property) ?? string.Empty;
                var newValue = targetComponent?.GetProperty(property) ?? string.Empty;
                result.AddChange(property, oldValue, newValue);
            }
        }
    }
}