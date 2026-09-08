(selector) => {
    let elements;
    try { elements = document.querySelectorAll(selector); }
    catch (_) { return {error: 'invalid CSS selector'}; }
    if (elements.length === 0) return {error: 'no element found'};
    if (elements.length !== 1) return {error: `${elements.length} elements found; use a unique selector`};
    const element = elements[0];
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility !== 'visible' ||
        (element.checkVisibility && !element.checkVisibility({checkOpacity: true, checkVisibilityCSS: true}))) {
        return {error: 'element is hidden or has no rendered area'};
    }
    return {x: rect.x + window.scrollX, y: rect.y + window.scrollY,
        width: Math.ceil(rect.width), height: Math.ceil(rect.height), scale: 1};
}
