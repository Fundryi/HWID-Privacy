using System.Collections.Generic;
using System.Threading.Tasks;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services.Interfaces
{
    public interface IComponentParser
    {
        Task<List<ComponentIdentifier>> ParseConfiguration(string configText);
        string GetComponentType(string sectionText);
    }
}