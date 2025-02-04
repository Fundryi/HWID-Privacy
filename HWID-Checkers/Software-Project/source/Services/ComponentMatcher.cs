using System.Collections.Generic;
using System.Linq;
using HWIDChecker.Services.Interfaces;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services
{
    public class ComponentMatcher : IComponentMatcher
    {
        private readonly Dictionary<string, IComponentIdentifierStrategy> _strategies;

        public ComponentMatcher(Dictionary<string, IComponentIdentifierStrategy> strategies)
        {
            _strategies = strategies;
        }

        public List<(ComponentIdentifier Base, ComponentIdentifier Target)> MatchComponents(
            List<ComponentIdentifier> baseComponents,
            List<ComponentIdentifier> targetComponents)
        {
            var results = new List<(ComponentIdentifier Base, ComponentIdentifier Target)>();
            var processedTargets = new HashSet<ComponentIdentifier>();

            // First, try to match using primary keys
            foreach (var baseComponent in baseComponents)
            {
                var primaryMatches = FindMatchesByPrimaryKey(baseComponent, targetComponents)
                    .Where(target => !processedTargets.Contains(target))
                    .ToList();

                if (primaryMatches.Any())
                {
                    var bestMatch = primaryMatches.First();
                    processedTargets.Add(bestMatch);
                    results.Add((baseComponent, bestMatch));
                }
                else
                {
                    // Try fallback matching if primary key matching failed
                    var fallbackMatches = FindMatchesByFallbackKey(baseComponent, targetComponents)
                        .Where(target => !processedTargets.Contains(target))
                        .ToList();

                    if (fallbackMatches.Any())
                    {
                        var bestMatch = fallbackMatches.First();
                        processedTargets.Add(bestMatch);
                        results.Add((baseComponent, bestMatch));
                    }
                    else
                    {
                        // No match found - component was removed
                        results.Add((baseComponent, null));
                    }
                }
            }

            // Add any remaining unmatched target components as new additions
            foreach (var target in targetComponents.Where(t => !processedTargets.Contains(t)))
            {
                results.Add((null, target));
            }

            return results;
        }

        private IEnumerable<ComponentIdentifier> FindMatchesByPrimaryKey(
            ComponentIdentifier component,
            List<ComponentIdentifier> candidates)
        {
            if (!_strategies.TryGetValue(component.Type, out var strategy))
            {
                return Enumerable.Empty<ComponentIdentifier>();
            }

            var baseId = strategy.GetIdentifier(component.Properties);
            if (string.IsNullOrEmpty(baseId))
            {
                return Enumerable.Empty<ComponentIdentifier>();
            }

            return candidates
                .Where(c => c.Type == component.Type)
                .Where(c => strategy.GetIdentifier(c.Properties) == baseId);
        }

        private IEnumerable<ComponentIdentifier> FindMatchesByFallbackKey(
            ComponentIdentifier component,
            List<ComponentIdentifier> candidates)
        {
            if (!_strategies.TryGetValue(component.Type, out var strategy))
            {
                return Enumerable.Empty<ComponentIdentifier>();
            }

            var baseFallbacks = strategy.GetFallbackIdentifiers(component.Properties);
            if (!baseFallbacks.Any())
            {
                return Enumerable.Empty<ComponentIdentifier>();
            }

            return candidates
                .Where(c => c.Type == component.Type)
                .Where(c =>
                {
                    var targetFallbacks = strategy.GetFallbackIdentifiers(c.Properties);
                    return baseFallbacks.Any(bf => targetFallbacks.Contains(bf));
                });
        }
    }
}