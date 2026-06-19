export function updateClock(clock) {
  clock.textContent = new Date().toLocaleTimeString([], { hour12: false });
}

export function startClock(clock) {
  updateClock(clock);
  return setInterval(() => updateClock(clock), 1000);
}
