export function OSLogoStatic() {
  return (
    <svg
      width="200"
      height="200"
      viewBox="0 0 200 200"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className="drop-shadow-lg"
    >
      {/* Outer ring - represents local device boundary */}
      <circle
        cx="100"
        cy="100"
        r="85"
        stroke="url(#ringGradient)"
        strokeWidth="2"
        opacity="0.4"
      />
      
      {/* Central core - the user/device at the center */}
      <circle
        cx="100"
        cy="100"
        r="24"
        fill="url(#coreGradient)"
      />
      
      {/* Peer nodes - distributed, equal, connected */}
      {[
        { angle: 0, color: '#60A5FA' },    // Blue
        { angle: 72, color: '#7C3AED' },   // Purple
        { angle: 144, color: '#EC4899' },  // Pink
        { angle: 216, color: '#10B981' },  // Green (was Violet)
        { angle: 288, color: '#3B82F6' }   // Deep Blue
      ].map(({ angle, color }) => {
        const radian = (angle * Math.PI) / 180;
        const x = 100 + 60 * Math.cos(radian);
        const y = 100 + 60 * Math.sin(radian);
        
        return (
          <g key={angle}>
            {/* Connection line from center to peer */}
            <line
              x1="100"
              y1="100"
              x2={x}
              y2={y}
              stroke="#F59E0B"
              strokeWidth="1.5"
              opacity="0.5"
              strokeDasharray="4 4"
            />
            
            {/* Peer node */}
            <circle
              cx={x}
              cy={y}
              r="12"
              fill={color}
            />
            
            {/* Highlight on peer node */}
            <circle
              cx={x - 3}
              cy={y - 3}
              r="3"
              fill="white"
              opacity="0.8"
            />
          </g>
        );
      })}
      
      {/* Stylized Crown symbol in center - representing sovereignty */}
      <g transform="translate(100, 100)">
        {/* Crown base - simple trapezoid */}
        <path
          d="M -10 4 L -12 8 L 12 8 L 10 4 Z"
          fill="url(#crownGradient)"
        />
        
        {/* Three simple peaks */}
        <path
          d="M -10 4 L -8 -8 L -6 4 M -2 4 L 0 -10 L 2 4 M 6 4 L 8 -8 L 10 4"
          fill="url(#crownGradient)"
        />
        
        {/* Simple shine line */}
        <line
          x1="-3"
          y1="2"
          x2="-1"
          y2="-2"
          stroke="white"
          strokeWidth="1.5"
          opacity="0.5"
          strokeLinecap="round"
        />
      </g>
      
      {/* Gradients */}
      <defs>
        <radialGradient id="coreGradient">
          <stop offset="0%" stopColor="#FCD34D" />
          <stop offset="50%" stopColor="#F59E0B" />
          <stop offset="100%" stopColor="#D97706" />
        </radialGradient>
        
        <linearGradient id="ringGradient" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#60A5FA" />
          <stop offset="50%" stopColor="#8B5CF6" />
          <stop offset="100%" stopColor="#EC4899" />
        </linearGradient>
        
        <linearGradient id="crownGradient" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stopColor="#CD7F32" />
          <stop offset="50%" stopColor="#A0522D" />
          <stop offset="100%" stopColor="#8B4513" />
        </linearGradient>
      </defs>
    </svg>
  );
}