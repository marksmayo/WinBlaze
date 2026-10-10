export function formatObservedChange(percentLower, metric) {
    const direction = percentLower < 0 ? 'higher' : 'lower';
    return `${Math.abs(percentLower).toFixed(1)}% ${direction} observed ${metric}`;
}
