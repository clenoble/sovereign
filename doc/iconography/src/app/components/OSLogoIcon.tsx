export function OSLogoIcon() {
  return (
    <svg
      width="32"
      height="32"
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Simplified central circle with crown */}
      <circle
        cx="16"
        cy="16"
        r="14"
        fill="url(#iconCoreGradient)"
      />
      
      {/* Very simple crown */}
      <g transform="translate(16, 16)">
        {/* Crown base */}
        <path
          d="M -5 2 L -6 4 L 6 4 L 5 2 Z"
          fill="url(#iconCrownGradient)"
        />
        
        {/* Three peaks - simplified */}
        <path
          d="M -5 2 L -4 -4 L -3 2 M -1 2 L 0 -5 L 1 2 M 3 2 L 4 -4 L 5 2"
          fill="url(#iconCrownGradient)"
        />
      </g>
      
      {/* Gradients */}
      <defs>
        <radialGradient id="iconCoreGradient">
          <stop offset="0%" stopColor="#FCD34D" />
          <stop offset="50%" stopColor="#F59E0B" />
          <stop offset="100%" stopColor="#D97706" />
        </radialGradient>
        
        <linearGradient id="iconCrownGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stopColor="#CD7F32" />
          <stop offset="50%" stopColor="#A0522D" />
          <stop offset="100%" stopColor="#8B4513" />
        </linearGradient>
      </defs>
    </svg>
  );
}
