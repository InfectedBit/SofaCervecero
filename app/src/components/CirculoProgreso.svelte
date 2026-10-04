<script lang="ts">
  /**
   * El círculo que se llena mientras se mantiene el botón (guión E.0_4 §9.1).
   *
   * No es adorno: es lo que convierte un gesto invisible en uno que se entiende. Mientras se
   * llena dice **qué va a pasar**, y soltar antes lo cancela sin consecuencias — que es la razón
   * de usar un mantenido y no una pulsación suelta para algo que cambia de pantalla.
   */
  let { progreso, texto }: { progreso: number; texto: string } = $props();

  const R = 34;
  const VUELTA = 2 * Math.PI * R;
  let recorrido = $derived(VUELTA * Math.max(0, Math.min(1, progreso)));
</script>

<div class="circulo-wrap" role="status" aria-label={texto}>
  <svg viewBox="0 0 80 80" class="circulo">
    <circle cx="40" cy="40" r={R} class="circulo-fondo" />
    <circle
      cx="40"
      cy="40"
      r={R}
      class="circulo-barra"
      stroke-dasharray="{recorrido} {VUELTA}"
    />
  </svg>
  <span class="circulo-texto">{texto}</span>
</div>
