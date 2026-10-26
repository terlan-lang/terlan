// Turn the shipped type checks into an application covering every host adapter.
export function consumerSource(source, entry) {
  const helpers = [];
  for (const member of entry.members ?? []) {
    if (member.kind !== 'mixin') continue;
    if (new RegExp(`pub\\s+${member.name}_typechecks(?:\\[[^\\]]*\\])?\\s*\\(`).test(source)) continue;
    if (member.return_type !== 'BindingTarget') throw new Error(`Unsupported mixin contract: ${entry.module}.${member.name}`);
    const names = (member.parameters ?? []).map((_, index) => `callback_${index}`);
    const parameters = ['binding_target: BindingTarget', ...(member.parameters ?? []).map((parameter, index) => `${names[index]}: ${parameter.type}`)];
    helpers.push(`pub ${member.name}_typechecks[BindingTarget](${parameters.join(', ')}): BindingTarget ->\n    ${entry.module}.${member.name}[BindingTarget](binding_target, ${names.join(', ')}).`);
  }
  if (!helpers.length) return source;
  return source.replace(/(^module\s+[\w.]+\.\s*$)/m, `$1\n\nimport ${entry.module}.`) + `\n\n${helpers.join('\n\n')}\n`;
}
