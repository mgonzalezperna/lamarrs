# Functional Requirements:
1. **Device Synchronization:**
   - The system should synchronize mobile devices to ensure coordinated actions during events.
   
2. **Text Broadcasting Control:**
   - The system must be able to broadcast text to the mobile devices remotely.
   
3. **Audiovisual Enhancements:**
   - The system should support audiovisual enhancements, such as synchronized music playing via the devices speakers with visual effects on mobile devices.
   
4. **No Interactivity:**
   - Mobile devices should not be interactive, as this is a tool for supporting the event, not to generate distractions.
   
# Non-Functional Requirements:
1. **Performance:**
   - The system should have low latency and high responsiveness to ensure and maintain synchronicity mobile devices.
   
2. **Portability:**
   - It is required for the server to have an extremely tiny footprint, while having an extreme flexibility to be ported to several different hardware architectures.
   
3. **Scalability:**
   - The infrastructure should be scalable to accommodate a large number of mobile devices and users simultaneously.
   
4. **Reliability:**
   - The system should be reliable, with minimal downtime or disruptions during events. Ideally, must have some level of self healing.

### Desirable:

5. **Usability:**
- The user interface of the control system should be intuitive and easy to use, catering to both event organizers and attendees.

6. **Security:**
   - Data transmission and communication between mobile devices and the centralized control system should be encrypted and secure to prevent unauthorized access or tampering.
   

# Domain Requirements:
1. **Network Connectivity:**
   - The system requires a stable and high-speed network connection to facilitate communication between mobile devices and the centralized control system. Using a exclusive WiFi is desirable.
   
2. **Power Usage:**
   - Adequate power management on the Client side is also required, as the mobile devices must remain functional throughout the entire duration of the event -and, ideally, afterwards too.

3. **Compatibility with Different Mobile Devices:**
   - The system should be compatible with a wide range of mobile devices across different operating systems (iOS, Android) and hardware specifications to ensure inclusivity and accessibility.

### Desirable:

4. **Integration with Event Management Systems:**
   - The system should allow integration with some event management systems to receive event schedules, timelines, and cues for synchronized lighting effects.

5. **Observability - Real-time Monitoring and Control:**
   - Event organizers should have real-time monitoring and control capabilities through the centralized system dashboard to adjust lighting settings and effects as needed during live events.
