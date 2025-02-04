using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using System.Threading;
using HWIDChecker.Services.Interfaces;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services
{
    public class CompareService
    {
        private readonly IComponentMatcher _matcher;
        private readonly IChangeDetector _changeDetector;
        private readonly Dictionary<string, IComponentIdentifierStrategy> _strategies;
        private readonly IComponentParser _parser;

        public CompareService(
            IComponentMatcher matcher,
            IChangeDetector changeDetector,
            Dictionary<string, IComponentIdentifierStrategy> strategies,
            IComponentParser parser)
        {
            _matcher = matcher;
            _changeDetector = changeDetector;
            _strategies = strategies;
            _parser = parser;
        }

        public async Task<List<ComparisonResult>> CompareConfigurations(
            string baseConfig,
            string targetConfig,
            CancellationToken cancellationToken = default)
        {
            // Parse configurations in parallel
            var parseBaseTask = Task.Run(() => _parser.Parse(baseConfig), cancellationToken);
            var parseTargetTask = Task.Run(() => _parser.Parse(targetConfig), cancellationToken);

            await Task.WhenAll(parseBaseTask, parseTargetTask);

            var baseComponents = parseBaseTask.Result;
            var targetComponents = parseTargetTask.Result;

            // Detect changes using our matching and change detection pipeline
            return await Task.Run(() =>
            {
                try
                {
                    // Match components using primary and fallback strategies
                    var matches = _matcher.MatchComponents(baseComponents, targetComponents);

                    // Detect changes in matched components
                    return _changeDetector.DetectChanges(matches);
                }
                catch (Exception ex)
                {
                    throw new ComparisonException("Failed to compare configurations", ex)
                    {
                        ErrorCode = "COMPARE_FAILED"
                    };
                }
            }, cancellationToken);
        }
    }

    public class ComparisonException : Exception
    {
        public string ErrorCode { get; set; }
        public string ComponentType { get; set; }

        public ComparisonException(string message, Exception innerException = null)
            : base(message, innerException)
        {
        }
    }
}