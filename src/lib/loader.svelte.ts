import { untrack } from "svelte";
import { errorMessage } from "./format";

/**
 * Async value holder. `run()` starts a load; only the latest call's result is
 * kept, so fast-changing inputs never show stale data. Previous data stays
 * visible while the next load is in flight.
 */
export class Loader<T> {
  data = $state.raw<T | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);
  #seq = 0;
  #last: (() => Promise<T>) | null = null;

  run(load: (() => Promise<T>) | null) {
    const seq = ++this.#seq;
    this.#last = load;
    if (!load) {
      this.data = null;
      this.error = null;
      this.loading = false;
      return;
    }
    this.loading = true;
    this.error = null;
    // Don't let anything read during the call become a dependency of the caller's effect.
    untrack(load).then(
      (d) => {
        if (seq !== this.#seq) return;
        this.data = d;
        this.loading = false;
      },
      (e) => {
        if (seq !== this.#seq) return;
        this.data = null;
        this.error = errorMessage(e);
        this.loading = false;
      },
    );
  }

  retry = () => this.run(this.#last);
}
