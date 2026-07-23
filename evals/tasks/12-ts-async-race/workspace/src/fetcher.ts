export async function fetchWithTimeout(url: string, ms: number): Promise<string> {
  // BUG: timeout is ignored — the setTimeout runs but never cancels the fetch
  const _timer = setTimeout(() => {}, ms);
  const response = await fetch(url);
  return response.text();
}
