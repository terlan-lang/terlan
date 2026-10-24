import compatibility from '@mdn/browser-compat-data/forLegacyNode';

function versionParts(version) {
  if (typeof version !== 'string' || !/^≤?\d+(?:\.\d+)*$/.test(version)) return null;
  return version.replace(/^≤/, '').split('.').map(Number);
}

function compareVersions(left, right) {
  const a = versionParts(left);
  const b = versionParts(right);
  if (!a || !b) return null;
  for (let index = 0; index < Math.max(a.length, b.length); index++) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference) return Math.sign(difference);
  }
  return 0;
}

export function releasedImplementations(feature, data = compatibility) {
  const implementations = [];
  for (const [browser, support] of Object.entries(feature?.support ?? {})) {
    const current = Object.entries(data.browsers[browser]?.releases ?? {})
      .filter(([, release]) => release.status === 'current')
      .map(([version]) => version)
      .sort((left, right) => compareVersions(right, left) ?? 0)[0];
    if (!current) continue;
    const statements = Array.isArray(support) ? support : [support];
    const implemented = statements.some(statement => {
      if (!statement || typeof statement !== 'object') return false;
      if (statement.prefix || statement.alternative_name || statement.flags?.length) return false;
      if (statement.version_added !== true) {
        const added = compareVersions(statement.version_added, current);
        if (added === null || added > 0) return false;
      }
      if (statement.version_removed) {
        const removed = compareVersions(statement.version_removed, current);
        if (removed === null || removed <= 0) return false;
      }
      return true;
    });
    if (implemented) implementations.push(browser);
  }
  return implementations.sort();
}

export function featureAdoption(feature, data = compatibility) {
  if (!feature) return { included: false, reason: 'no-compatibility-data' };
  if (feature.status?.standard_track !== true) return { included: false, reason: 'not-standards-track' };
  const browsers = releasedImplementations(feature, data);
  return browsers.length
    ? { included: true, browsers }
    : { included: false, reason: 'no-released-browser-implementation' };
}

export function propertyAdoption(name, data = compatibility) {
  return featureAdoption(data.css.properties[name]?.__compat, data);
}

// The spec snapshot can contain proposed additions to already shipped
// properties. Prefer explicit value support; otherwise require the value to
// occur in MDN's independently maintained property grammar.
export function atomAdopted(name, atom, grammarAtoms, data = compatibility) {
  const property = data.css.properties[name];
  if (atom.kind === 'keyword') {
    const feature = property?.[atom.name]?.__compat;
    if (feature) return featureAdoption(feature, data).included;
    return grammarAtoms?.some(candidate => candidate.kind === atom.kind && candidate.name === atom.name) ?? false;
  }
  if (grammarAtoms) return grammarAtoms.some(candidate => candidate.kind === atom.kind && candidate.name === atom.name);
  return featureAdoption(data.css.types[atom.name]?.__compat, data).included;
}
