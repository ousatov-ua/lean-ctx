/**
 * Area tab strip (GL #487): every four-jobs area is one page whose views are
 * tabs. This element listens for `lctx:view` and renders the sibling tabs of
 * the active view; outside an area (Home) it stays hidden. Tabs are plain
 * hash links (`#area/tab`), so middle-click/copy-link keep working and the
 * router's lazy per-view loaders stay untouched.
 */

class CockpitAreaTabs extends HTMLElement {
  connectedCallback() {
    if (this._ready) return;
    this._ready = true;
    this._onViewEvent = this._onViewEvent.bind(this);
    document.addEventListener('lctx:view', this._onViewEvent);
    this.hidden = true;
  }

  disconnectedCallback() {
    document.removeEventListener('lctx:view', this._onViewEvent);
  }

  _onViewEvent(e) {
    const d = e.detail || {};
    if (!d.areaId || !window.LctxRouter || !window.LctxRouter.COCKPIT_AREAS) {
      this.hidden = true;
      this.innerHTML = '';
      return;
    }
    this._render(d.areaId, d.viewId);
  }

  _render(areaId, activeViewId) {
    const areas = window.LctxRouter.COCKPIT_AREAS;
    let area = null;
    for (let i = 0; i < areas.length; i++) {
      if (areas[i].id === areaId) { area = areas[i]; break; }
    }
    if (!area) {
      this.hidden = true;
      return;
    }
    let html =
      '<div class="area-tabs" role="tablist" aria-label="' +
      area.label.replace(/"/g, '&quot;') + ' tabs">';
    for (let i = 0; i < area.tabs.length; i++) {
      const t = area.tabs[i];
      const on = t.view === activeViewId;
      html +=
        '<a class="area-tab' + (on ? ' active' : '') + '" role="tab" ' +
        'aria-selected="' + (on ? 'true' : 'false') + '" ' +
        'href="#' + area.id + '/' + t.tab + '" data-view="' + t.view + '">' +
        t.label + '</a>';
    }
    html += '</div>';
    this.innerHTML = html;
    this.hidden = false;
    this._bindKeys();
  }

  // Roving arrow-key support per WAI-ARIA tabs pattern; activation stays on
  // click/Enter because tabs are links and switching loads data lazily.
  _bindKeys() {
    const tabs = [...this.querySelectorAll('.area-tab')];
    tabs.forEach(function (tab, idx) {
      tab.addEventListener('keydown', function (e) {
        let next = -1;
        if (e.key === 'ArrowRight') next = (idx + 1) % tabs.length;
        else if (e.key === 'ArrowLeft') next = (idx - 1 + tabs.length) % tabs.length;
        else if (e.key === 'Home') next = 0;
        else if (e.key === 'End') next = tabs.length - 1;
        if (next >= 0) {
          e.preventDefault();
          tabs[next].focus();
        }
      });
    });
  }
}

customElements.define('cockpit-area-tabs', CockpitAreaTabs);

export { CockpitAreaTabs };
