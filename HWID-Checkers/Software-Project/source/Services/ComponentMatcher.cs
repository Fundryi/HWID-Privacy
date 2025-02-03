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

            foreach (var baseComponent in baseComponents)
            {
                var matches = FindMatches(baseComponent, targetComponents)
                    .Where(target => !processedTargets.Contains(target))
                    .ToList();

                if (matches.Any())
                {
                    var bestMatch = matches.First();
                    processedTargets.Add(bestMatch);
                    results.Add((baseComponent, bestMatch));
                }
                else
                {
                    // No match found - component was removed
                    results.Add((baseComponent, null));
                }
            }

            // Add any remaining unmatched target components as new additions
            foreach (var target in targetComponents.Where(t => !processedTargets.Contains(t)))
            {
                results.Add((null, target));
            }

            return results;
        }

        private IEnumerable<ComponentIdentifier> FindMatches(
            ComponentIdentifier component,
            List<ComponentIdentifier> candidates)
        {
            // Only match components of the same type
            return candidates.Where(c => c.Type == component.Type);
        }
    }
}