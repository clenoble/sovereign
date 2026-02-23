export function OSLogoBW() {
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
        stroke="white"
        strokeWidth="2"
        opacity="0.4"
      />
      
      {/* Central core - the user/device at the center */}
      <circle
        cx="100"
        cy="100"
        r="24"
        fill="white"
      />
      
      {/* Peer nodes - distributed, equal, connected */}
      {[0, 72, 144, 216, 288].map((angle) => {
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
              stroke="white"
              strokeWidth="1.5"
              opacity="0.5"
              strokeDasharray="4 4"
            />
            
            {/* Peer node */}
            <circle
              cx={x}
              cy={y}
              r="12"
              fill="white"
              opacity="0.8"
            />
            
            {/* Highlight on peer node */}
            <circle
              cx={x - 3}
              cy={y - 3}
              r="3"
              fill="white"
            />
          </g>
        );
      })}
      
      {/* Stylized Crown symbol in center - representing sovereignty */}
      <g transform="translate(100, 100)">
        {/* Crown base - simple trapezoid */}
        <path
          d="M -10 4 L -12 8 L 12 8 L 10 4 Z"
          fill="black"
          opacity="0.7"
        />
        
        {/* Three simple peaks */}
        <path
          d="M -10 4 L -8 -8 L -6 4 M -2 4 L 0 -10 L 2 4 M 6 4 L 8 -8 L 10 4"
          fill="black"
          opacity="0.7"
        />
        
        {/* Simple shine line */}
        <line
          x1="-3"
          y1="2"
          x2="-1"
          y2="-2"
          stroke="black"
          strokeWidth="1.5"
          opacity="0.5"
          strokeLinecap="round"
        />
      </g>
    </svg>
  );
}
