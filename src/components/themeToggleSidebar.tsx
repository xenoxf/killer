import { createSignal, createEffect, onCleanup, Show } from "solid-js";
import { useTheme, themes } from "../contexts/ThemeContext";
import styles from "../styles/themeToggleSidebar.module.css";

export function ThemeToggleSidebar() {
  const { theme, setTheme } = useTheme();
  const [open, setOpen] = createSignal(false);
  let ref: HTMLDivElement | undefined;

  const close = () => setOpen(false);

  const onPointerDownOutside = (e: PointerEvent) => {
    if (ref && !ref.contains(e.target as Node)) close();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };

  // `pointerdown` (no `click`) para no competir con el `onClick` delegado
  // de Solid: el pointerdown fuera cierra antes del click, y el click del
  // botón hace toggle sin que el listener lo cancele (el botón está dentro).
  createEffect(() => {
    if (open()) {
      document.addEventListener("pointerdown", onPointerDownOutside);
      document.addEventListener("keydown", onKeyDown);
    } else {
      document.removeEventListener("pointerdown", onPointerDownOutside);
      document.removeEventListener("keydown", onKeyDown);
    }
  });
  onCleanup(() => {
    document.removeEventListener("pointerdown", onPointerDownOutside);
    document.removeEventListener("keydown", onKeyDown);
  });

  return (
    <div class={styles.container} ref={(el) => (ref = el)}>
      <button
        type="button"
        class={`${styles.themeToggle} ${styles.themeToggleCollapsed}`}
        classList={{ [styles.open]: open() }}
        title="Cambiar tema"
        aria-haspopup="listbox"
        aria-expanded={open()}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        {/* Icono inline (sin dependencia de red): paleta. Antes era un
            <img> a img.icons8.com que no cargaba offline/en Tauri y el
            botón parecía "no funcionar". */}
        <svg
          width="22"
          height="22"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="13.5" cy="6.5" r=".5" fill="currentColor" />
          <circle cx="17.5" cy="10.5" r=".5" fill="currentColor" />
          <circle cx="8.5" cy="7.5" r=".5" fill="currentColor" />
          <circle cx="6.5" cy="12.5" r=".5" fill="currentColor" />
          <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h2.36C19.583 16.594 22 13.206 22 10c0-4.418-4.418-8-10-8z" />
        </svg>
      </button>

      <Show when={open()}>
        <div class={styles.dropdown} role="listbox" aria-label="Temas">
          <div class={styles.dropdownHeader}>Temas</div>
          <div class={styles.dropdownList}>
            {themes.map((t) => {
              const active = theme() === t.name;
              return (
                <button
                  type="button"
                  role="option"
                  aria-selected={active}
                  class={`${styles.dropdownItem} ${
                    active ? styles.activeItem : ""
                  }`}
                  onClick={() => {
                    setTheme(t.name);
                    close();
                  }}
                >
                  <div
                    class={styles.previewSmall}
                    style={{ background: t.preview }}
                  />
                  <span class={styles.itemLabel}>{t.label}</span>
                  {active && <span class={styles.check}>✓</span>}
                </button>
              );
            })}
          </div>
        </div>
      </Show>
    </div>
  );
}

export default ThemeToggleSidebar;
