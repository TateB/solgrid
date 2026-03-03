// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Events {
    event Transfer(  address indexed  from,  address indexed  to,  uint256  amount  );
    event Approval(address indexed owner,address indexed spender,uint256 amount);
    event Deposit(  address indexed user, uint256 amount, uint256 timestamp  );

    error InsufficientBalance(  uint256 requested,uint256 available  );
    error Unauthorized(  );
    error InvalidAmount(uint256 amount);

    struct UserInfo {
        uint256 balance;
        uint256 lastAction;
        bool isActive;
    }

    enum Status { Active, Paused, Stopped }

    mapping(address => UserInfo) public users;

    function deposit() external payable {
        users[msg.sender].balance += msg.value;
        users[msg.sender].lastAction = block.timestamp;
        users[msg.sender].isActive = true;
        emit Deposit(msg.sender, msg.value, block.timestamp);
    }
}
