// Esimerkkilaajennus: näyttää pienen viestin jokaisen https-sivun oikeassa alakulmassa.
const note = document.createElement('div');
note.textContent = 'Hei Viimasta!';
note.style.cssText =
    'position:fixed;bottom:8px;right:8px;padding:4px 8px;background:#111;color:#fff;z-index:2147483647';
document.body.appendChild(note);
