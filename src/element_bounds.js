(selector) => {
    let elements;
    try { elements = document.querySelectorAll(selector); }
    catch (_) { return {error: 'CSS selector inválido'}; }
    if (elements.length === 0) return {error: 'nenhum elemento encontrado'};
    if (elements.length !== 1) return {error: `${elements.length} elementos encontrados; use um seletor único`};
    const element = elements[0];
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility !== 'visible' ||
        (element.checkVisibility && !element.checkVisibility({checkOpacity: true, checkVisibilityCSS: true}))) {
        return {error: 'elemento oculto ou sem área renderizada'};
    }
    return {x: rect.x + window.scrollX, y: rect.y + window.scrollY,
        width: Math.ceil(rect.width), height: Math.ceil(rect.height), scale: 1};
}
