// All consumers must compile before launching the browser matrix.
export function verifyCompilations(results) {
  if (!Array.isArray(results) || results.length === 0) throw new Error('No browser compiler results were produced');
  const failures = results.filter(result => result.status !== 0 || result.signal);
  if (failures.length) {
    throw new Error(`Terlan browser compilation failed:\n${failures.map(result => `${result.module}: ${result.signal ?? `status ${result.status}`}\n${result.diagnostics ?? ''}`).join('\n')}`);
  }
}
